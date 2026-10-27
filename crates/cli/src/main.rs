#[cfg(feature = "mimalloc")]
#[global_allocator]
static ALLOC: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[cfg(feature = "dhat-heap")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

use callistseo_export as export;

use callistseo_core::{normalize, CrawlConfig, CrawledPage, Issue, Severity};
use callistseo_seo_rules::{analyze, RulesConfig};
use callistseo_storage::Store;
use clap::{Parser, Subcommand, ValueEnum};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[derive(Parser)]
#[command(name = "callistseo", version, about = "Audit SEO open source")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Explore un site et liste les URL découvertes
    Crawl {
        url: String,
        #[arg(long, default_value_t = 1000)]
        max_pages: usize,
        #[arg(long, default_value_t = 10)]
        max_depth: usize,
        /// Requêtes en parallèle, fixes. Sans cette option : adaptatif (de 8 à `--max-concurrency`,
        /// 16 par défaut), il monte tant que le serveur répond vite et recule sur 429/5xx.
        #[arg(long)]
        concurrency: Option<usize>,
        /// Plafond de la concurrence adaptative (avec `--concurrency` : borne haute, sinon 16)
        #[arg(long)]
        max_concurrency: Option<usize>,
        /// Délai minimal entre deux requêtes, en ms (le Crawl-delay de robots.txt s'applique aussi)
        #[arg(long, default_value_t = 0)]
        delay_ms: u64,
        /// Vérifie aussi les liens vers d'autres sites (requêtes vers des tiers : au plus 500 URL, robots.txt et délai respectés)
        #[arg(long)]
        check_external: bool,
        /// Autorise les IP privées/loopback (tests locaux uniquement)
        #[arg(long)]
        allow_private: bool,
        /// Ne pas consulter robots.txt
        #[arg(long)]
        ignore_robots: bool,
        /// Identifiant du robot, ex. "CallistSEO/0.1 (+https://votre-site/bot)" (sinon la variable
        /// CALLISTSEO_USER_AGENT, sinon `CallistSEO/<version>`). Le nom avant « / » est ce que
        /// robots.txt cible : le changer change les règles qui s'appliquent.
        #[arg(long)]
        user_agent: Option<String>,
        /// Ne suivre que les URL qui correspondent à cette expression régulière (répétable)
        #[arg(long)]
        include: Vec<String>,
        /// Ne jamais suivre les URL qui correspondent à cette expression régulière (répétable)
        #[arg(long)]
        exclude: Vec<String>,
        /// Paramètre de requête à ignorer, `utm_*` pour un préfixe (répétable)
        #[arg(long = "strip-param")]
        strip_params: Vec<String>,
        /// Ne pas suivre les URL plus longues que ce nombre de caractères
        #[arg(long)]
        max_url_length: Option<usize>,
        /// Extraction personnalisée `nom: type: motif` (type : regex, css, css@attribut, contains) ; répétable
        #[arg(long = "extract")]
        extract: Vec<String>,
        /// Fichier robots.txt à utiliser à la place de celui du site
        #[arg(long)]
        robots_file: Option<PathBuf>,
        /// Gravité à partir de laquelle la commande échoue (code retour 1) : utile en CI
        #[arg(long, value_enum, default_value_t = FailOn::Error)]
        fail_on: FailOn,
        /// Format de sortie
        #[arg(long, value_enum, default_value_t = Format::Text)]
        format: Format,
        /// Dossier de sortie : pages.csv + issues.csv (csv, obligatoire) ou report.json (json, sinon stdout)
        #[arg(long)]
        out: Option<PathBuf>,
        /// Enregistre l'audit dans cette base SQLite (créée si besoin)
        #[arg(long)]
        db: Option<PathBuf>,
        /// Nom du projet pour l'historique (défaut : le nom d'hôte)
        #[arg(long, requires = "db")]
        project: Option<String>,
    },
    /// Liste les règles d'analyse avec leur gravité
    Rules,
    /// Liste les audits enregistrés dans une base
    Runs {
        #[arg(long)]
        db: PathBuf,
    },
    /// Réexporte un audit enregistré
    Export {
        #[arg(long)]
        db: PathBuf,
        /// Identifiant de l'exécution (voir `runs`)
        #[arg(long)]
        run: i64,
        #[arg(long, value_enum, default_value_t = Format::Text)]
        format: Format,
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Lance l'API HTTP (projets, audits en arrière-plan, résultats, exports)
    Serve {
        #[arg(long)]
        db: PathBuf,
        /// Adresse d'écoute ; hors loopback, un jeton est obligatoire
        #[arg(long, default_value = "127.0.0.1:8080")]
        addr: std::net::SocketAddr,
        /// Jeton Bearer (16 caractères minimum) ; sinon la variable CALLISTSEO_TOKEN
        #[arg(long)]
        token: Option<String>,
        /// Dossier de l'interface construite (apps/web/dist) à servir sur `/`
        #[arg(long)]
        static_dir: Option<PathBuf>,
        /// Identifiant du robot pour tous les audits (sinon CALLISTSEO_USER_AGENT, sinon par défaut)
        #[arg(long)]
        user_agent: Option<String>,
        /// Derrière un proxy HTTPS : l'adresse du client vient de X-Forwarded-For (journal, limite
        /// des tentatives de connexion) ; sinon la variable CALLISTSEO_TRUST_PROXY=1
        #[arg(long)]
        trust_proxy: bool,
    },
    /// Serveur MCP sur stdio : un assistant lance des audits et interroge les résultats
    Mcp {
        /// Base SQLite des audits (créée si absente)
        #[arg(long)]
        db: PathBuf,
        /// Identifiant du robot pour tous les audits (sinon CALLISTSEO_USER_AGENT, sinon par défaut)
        #[arg(long)]
        user_agent: Option<String>,
    },
    /// Compare deux audits enregistrés
    Diff {
        #[arg(long)]
        db: PathBuf,
        /// Audit de référence
        #[arg(long)]
        before: i64,
        /// Audit à comparer
        #[arg(long)]
        after: i64,
    },
}

/// Compare deux audits enregistrés ; code retour 1 si de nouvelles erreurs sont apparues
fn diff_cmd(db: &Path, before: i64, after: i64) -> Result<ExitCode, String> {
    let store = Store::open(db)?;
    let (bp, bi) = store.load_run(before)?;
    let (ap, ai) = store.load_run(after)?;
    let d = callistseo_audit_diff::diff((&bp, &bi), (&ap, &ai));
    for u in &d.added_pages {
        println!("+ page\t{u}");
    }
    for u in &d.removed_pages {
        println!("- page\t{u}");
    }
    for c in &d.changed_pages {
        for ch in &c.changes {
            println!(
                "~ {}\t{}\t{:?} -> {:?}",
                ch.field, c.url, ch.before, ch.after
            );
        }
    }
    for i in &d.new_issues {
        println!(
            "NOUVEAU\t{:?}\t{}\t{}\t{}",
            i.severity, i.rule, i.url, i.message
        );
    }
    for i in &d.resolved_issues {
        println!(
            "RÉSOLU\t{:?}\t{}\t{}\t{}",
            i.severity, i.rule, i.url, i.message
        );
    }
    let regressions = d
        .new_issues
        .iter()
        .filter(|i| i.severity == Severity::Error)
        .count();
    eprintln!(
        "#{before} -> #{after} : {} pages ajoutées, {} supprimées, {} modifiées ; {} problèmes nouveaux ({regressions} erreurs), {} résolus",
        d.added_pages.len(),
        d.removed_pages.len(),
        d.changed_pages.len(),
        d.new_issues.len(),
        d.resolved_issues.len()
    );
    Ok(ExitCode::from(u8::from(regressions > 0)))
}

/// Seuil d'échec de `crawl`.
#[derive(Clone, Copy, ValueEnum)]
enum FailOn {
    /// Au moins une erreur (défaut) : quelque chose est cassé
    Error,
    /// Au moins une erreur ou un avertissement
    Warning,
    /// Ne jamais échouer à cause des constats
    Never,
}

#[derive(Clone, Copy, ValueEnum)]
enum Format {
    Text,
    Csv,
    Json,
    /// Rapport autonome (un seul fichier HTML, sans script)
    Html,
    /// Sitemap XML des pages indexables (sitemap.xml, ou sitemap-1.xml… au-delà de 50 000 URL)
    Sitemap,
}

/// `nom: type: motif`, où `type` vaut `regex`, `css`, `css@attribut` ou `contains`.
fn parse_extractor(spec: &str) -> Result<callistseo_core::Extractor, String> {
    use callistseo_core::ExtractKind;
    let mut parts = spec.splitn(3, ':');
    let (name, kind, pattern) = (parts.next(), parts.next(), parts.next());
    let (Some(name), Some(kind), Some(pattern)) = (name, kind, pattern) else {
        return Err(format!(
            "--extract « {spec} » : format attendu `nom: type: motif`"
        ));
    };
    let (kind, attr) = match kind.trim().split_once('@') {
        Some((k, a)) => (k, Some(a.trim().to_owned())),
        None => (kind.trim(), None),
    };
    let kind = match kind {
        "regex" => ExtractKind::Regex,
        "css" => ExtractKind::Css,
        "contains" => ExtractKind::Contains,
        other => {
            return Err(format!(
                "--extract : type « {other} » inconnu (regex, css, contains)"
            ))
        }
    };
    Ok(callistseo_core::Extractor {
        name: name.trim().to_owned(),
        kind,
        pattern: pattern.trim().to_owned(),
        attr,
    })
}

fn check_out(format: Format, out: &Option<PathBuf>) -> Result<(), String> {
    if matches!(format, Format::Csv | Format::Sitemap) && out.is_none() {
        return Err("--format csv et --format sitemap demandent --out <dossier>".into());
    }
    Ok(())
}

fn output(
    format: Format,
    out: Option<&Path>,
    meta: &export::ReportMeta,
    pages: &[CrawledPage],
    issues: &[Issue],
) -> std::io::Result<()> {
    let write = |name: &str, content: String| -> std::io::Result<()> {
        let dir = out.expect("dossier vérifié avant le crawl");
        std::fs::create_dir_all(dir)?;
        std::fs::write(dir.join(name), content)
    };
    match (format, out) {
        (Format::Csv, _) => {
            write("pages.csv", export::pages_csv(pages))?;
            write("issues.csv", export::issues_csv(issues))
        }
        (Format::Sitemap, _) => {
            let parts = export::sitemap_parts(pages);
            if let [only] = parts.as_slice() {
                return write("sitemap.xml", only.clone());
            }
            for (i, part) in parts.into_iter().enumerate() {
                write(&format!("sitemap-{}.xml", i + 1), part)?;
            }
            Ok(())
        }
        (Format::Html, Some(_)) => write("report.html", export::report_html(meta, pages, issues)),
        (Format::Html, None) => {
            println!("{}", export::report_html(meta, pages, issues));
            Ok(())
        }
        (Format::Json, Some(_)) => write("report.json", export::report_json(pages, issues)),
        (Format::Json, None) => {
            println!("{}", export::report_json(pages, issues));
            Ok(())
        }
        (Format::Text, _) => {
            for p in pages {
                let status = p.status.map_or("ERR".to_string(), |s| s.to_string());
                let note = p
                    .error
                    .as_deref()
                    .or(p.redirect_to.as_ref().map(|r| r.as_str()))
                    .unwrap_or("");
                let title = p.meta.title.as_deref().unwrap_or("");
                println!(
                    "{status}\t{}\t{}\t{note}\th1={}\t{title}",
                    p.depth,
                    p.url,
                    p.meta.h1.len()
                );
            }
            for i in issues {
                println!("{:?}\t{}\t{}\t{}", i.severity, i.rule, i.url, i.message);
            }
            Ok(())
        }
    }
}

/// `--user-agent`, sinon `CALLISTSEO_USER_AGENT` (vide = non défini), sinon l'identifiant par défaut.
fn resolve_user_agent(flag: Option<String>) -> String {
    flag.or_else(|| std::env::var("CALLISTSEO_USER_AGENT").ok())
        .filter(|ua| !ua.trim().is_empty())
        .unwrap_or_else(callistseo_core::default_user_agent)
}

/// Résumé sur stderr ; code retour 1 si le seuil `--fail-on` est atteint (exploitable en CI).
fn summary(pages: &[CrawledPage], issues: &[Issue], fail_on: FailOn) -> ExitCode {
    let count = |s| issues.iter().filter(|i| i.severity == s).count();
    let (errors, warnings) = (count(Severity::Error), count(Severity::Warning));
    eprintln!(
        "{} URL, {errors} erreurs, {warnings} avertissements, {} infos",
        pages.len(),
        count(Severity::Info)
    );
    let failed = match fail_on {
        FailOn::Error => errors > 0,
        FailOn::Warning => errors + warnings > 0,
        FailOn::Never => false,
    };
    ExitCode::from(u8::from(failed))
}

async fn run(cmd: Command) -> Result<ExitCode, String> {
    match cmd {
        Command::Crawl {
            url,
            max_pages,
            max_depth,
            concurrency,
            max_concurrency,
            delay_ms,
            check_external,
            allow_private,
            ignore_robots,
            user_agent,
            fail_on,
            format,
            out,
            db,
            project,
            include,
            exclude,
            strip_params,
            max_url_length,
            extract,
            robots_file,
        } => {
            check_out(format, &out)?;
            let robots_override = robots_file
                .map(|f| std::fs::read_to_string(&f).map_err(|e| format!("{} : {e}", f.display())))
                .transpose()?;
            let start = normalize(&url, None)
                .ok_or_else(|| format!("URL invalide (http/https attendu): {url}"))?;
            let cfg = CrawlConfig {
                max_pages,
                max_depth,
                concurrency: concurrency.unwrap_or(8).max(1),
                // `--concurrency N` seul = fixe ; `--max-concurrency M` autorise la montée jusqu'à M.
                max_concurrency: match (concurrency, max_concurrency) {
                    (Some(c), None) => c.max(1),
                    (c, Some(m)) => m.max(c.unwrap_or(8)).max(1),
                    (None, None) => 16,
                },
                allow_private,
                respect_robots: !ignore_robots,
                user_agent: resolve_user_agent(user_agent),
                delay: std::time::Duration::from_millis(delay_ms),
                scope: callistseo_core::UrlScope {
                    include,
                    exclude,
                    strip_params,
                    max_url_length,
                    robots_override,
                },
                extractors: extract
                    .iter()
                    .map(|e| parse_extractor(e))
                    .collect::<Result<_, _>>()?,
                check_external,
                ..CrawlConfig::default()
            };
            let mut pages = callistseo_crawler::crawl(start.clone(), cfg.clone()).await?;
            pages.sort_by(|a, b| (a.depth, &a.url).cmp(&(b.depth, &b.url)));
            let issues = analyze(&pages, &RulesConfig::default());
            let name = project.unwrap_or_else(|| start.host_str().unwrap_or("site").to_owned());
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs() as i64);
            let meta = export::ReportMeta {
                title: &name,
                start_url: start.as_str(),
                generated_at: &export::iso_utc(now),
            };
            output(format, out.as_deref(), &meta, &pages, &issues)
                .map_err(|e| format!("écriture impossible: {e}"))?;
            if let Some(db) = db {
                let id = Store::open(&db)?.save_run(&name, &start, &cfg, &pages, &issues)?;
                eprintln!("exécution #{id} enregistrée dans {}", db.display());
            }
            Ok(summary(&pages, &issues, fail_on))
        }
        Command::Rules => {
            for r in callistseo_seo_rules::RULES {
                println!("{:?}\t{}\t{}", r.severity, r.id, r.label);
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::Runs { db } => {
            for r in Store::open(&db)?.runs()? {
                println!(
                    "#{}\t{}\t{}\t{}\t{} URL\t{} erreurs\t{} avertissements",
                    r.id,
                    export::iso_utc(r.started_at),
                    r.project,
                    r.start_url,
                    r.pages,
                    r.errors,
                    r.warnings
                );
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::Diff { db, before, after } => diff_cmd(&db, before, after),
        Command::Serve {
            db,
            addr,
            token,
            static_dir,
            user_agent,
            trust_proxy,
        } => {
            let token = token.or_else(|| std::env::var("CALLISTSEO_TOKEN").ok());
            let cfg = callistseo_api::ApiConfig {
                token,
                static_dir,
                trust_proxy: trust_proxy
                    || std::env::var("CALLISTSEO_TRUST_PROXY").is_ok_and(|v| v == "1"),
                user_agent: resolve_user_agent(user_agent),
                ..Default::default()
            };
            callistseo_api::serve(addr, Store::open(&db)?, cfg).await?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Mcp { db, user_agent } => {
            let mut store = Store::open(&db)?;
            store.fail_stale_runs()?;
            let cfg = callistseo_api::ApiConfig {
                user_agent: resolve_user_agent(user_agent),
                ..Default::default()
            };
            eprintln!("serveur MCP CallistSEO prêt (stdio), base {}", db.display());
            callistseo_mcp::serve_stdio(callistseo_mcp::McpServer::new(
                callistseo_api::LocalApi::new(store, cfg),
            ))
            .await?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Export {
            db,
            run,
            format,
            out,
        } => {
            check_out(format, &out)?;
            let store = Store::open(&db)?;
            let info = store
                .run(run)?
                .ok_or_else(|| format!("exécution {run} introuvable"))?;
            let (pages, issues) = store.load_run(run)?;
            let meta = export::ReportMeta {
                title: &info.project,
                start_url: &info.start_url,
                generated_at: &export::iso_utc(info.started_at),
            };
            output(format, out.as_deref(), &meta, &pages, &issues)
                .map_err(|e| format!("écriture impossible: {e}"))?;
            Ok(ExitCode::SUCCESS)
        }
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    #[cfg(feature = "dhat-heap")]
    let _profiler = dhat::Profiler::new_heap();
    match run(Cli::parse().command).await {
        Ok(code) => code,
        Err(e) => {
            eprintln!("erreur: {e}");
            ExitCode::from(2)
        }
    }
}
