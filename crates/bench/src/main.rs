//! Benchmarks reproductibles : le même site synthétique, le vrai binaire `callistseo` en processus
//! séparé (mémoire maximale mesurée par le système), débit, latence, et exactitude des règles
//! comparée à une vérité terrain exacte.

mod site;

use callistseo_core::CrawlConfig;
use clap::{Parser, Subcommand};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::time::{Duration, Instant};
use tokio::net::TcpListener;

#[derive(Parser)]
#[command(
    name = "callistseo-bench",
    about = "Benchmarks reproductibles de CallistSEO"
)]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Lance la suite et produit le rapport Markdown
    Run {
        /// Binaire `callistseo` à mesurer (défaut : celui qui est à côté de ce binaire)
        #[arg(long)]
        cli: Option<PathBuf>,
        /// Suite réduite (vérification rapide)
        #[arg(long)]
        quick: bool,
        /// Écrit le rapport dans ce fichier (sinon : sortie standard)
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Sert le site synthétique, pour mesurer n'importe quel autre crawler sur le même corpus
    Serve {
        #[arg(long, default_value_t = 10_000)]
        pages: usize,
        #[arg(long, default_value_t = 8080)]
        port: u16,
        /// Latence simulée par réponse
        #[arg(long, default_value_t = 0)]
        delay_ms: u64,
        /// Pages denses (~250 Ko, ~300 liens), comme une page de presse
        #[arg(long)]
        dense: bool,
    },
    /// Test d'endurance : enchaîne des audits en processus et vérifie que la mémoire se stabilise
    Soak {
        #[arg(long, default_value_t = 8)]
        runs: usize,
        #[arg(long, default_value_t = 400)]
        pages: usize,
    },
}

/// Mémoire maximale du processus courant, en Mo.
fn max_rss_mb() -> f64 {
    let mut usage = unsafe { std::mem::zeroed::<libc::rusage>() };
    unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) };
    let unit = if cfg!(target_os = "macos") {
        1.0
    } else {
        1024.0
    };
    usage.ru_maxrss as f64 * unit / 1_048_576.0
}

/// La mémoire maximale ne peut que monter : si elle plafonne après les premiers audits, rien ne fuit.
async fn soak(runs: usize, pages: usize) -> Result<(), String> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let server = tokio::spawn(site::serve(
        listener,
        pages,
        Duration::ZERO,
        site::Corpus::DENSE,
    ));
    let start =
        url::Url::parse(&format!("http://127.0.0.1:{port}/p/0")).map_err(|e| e.to_string())?;
    let mut series = Vec::new();
    for i in 1..=runs {
        let cfg = CrawlConfig {
            allow_private: true,
            max_pages: pages + pages / 40 + 10,
            max_depth: 100,
            ..CrawlConfig::default()
        };
        let crawled = callistseo_crawler::crawl(start.clone(), cfg).await?;
        let issues =
            callistseo_seo_rules::analyze(&crawled, &callistseo_seo_rules::RulesConfig::default());
        drop((crawled, issues));
        series.push(max_rss_mb());
        eprintln!("audit {i}/{runs} : pic mémoire {:.1} Mo", series[i - 1]);
    }
    server.abort();
    // Le premier audit chauffe les allocateurs ; on compare le deuxième au dernier.
    let (second, last) = (series[1.min(runs - 1)], series[runs - 1]);
    if last > second * 1.15 {
        return Err(format!(
            "la mémoire continue de monter : {second:.1} Mo → {last:.1} Mo"
        ));
    }
    eprintln!("mémoire stable : {second:.1} Mo → {last:.1} Mo");
    Ok(())
}

#[derive(Clone, Copy)]
struct Scenario {
    group: &'static str,
    dense: bool,
    pages: usize,
    concurrency: usize,
    delay_ms: u64,
}

fn suite(quick: bool) -> Vec<Scenario> {
    let s = |group, pages, concurrency, delay_ms| Scenario {
        group,
        dense: false,
        pages,
        concurrency,
        delay_ms,
    };
    if quick {
        return vec![
            s("taille", 500, 8, 0),
            s("latence réseau", 300, 8, 10),
            dense(s("pages denses", 200, 8, 0)),
        ];
    }
    vec![
        s("taille", 1_000, 8, 0),
        s("taille", 10_000, 8, 0),
        s("taille", 50_000, 8, 0),
        s("concurrence", 10_000, 1, 0),
        s("concurrence", 10_000, 4, 0),
        s("concurrence", 10_000, 16, 0),
        s("concurrence", 10_000, 64, 0),
        s("latence réseau", 2_000, 8, 20),
        s("latence réseau", 2_000, 32, 20),
        // Pages de presse : ~250 Ko et ~300 liens (mémoire et coût d'analyse réalistes).
        dense(s("pages denses", 1_000, 8, 0)),
        dense(s("pages denses", 3_000, 8, 0)),
        dense(s("pages denses, latence 500 ms", 400, 8, 500)),
        dense(s("pages denses, latence 500 ms", 400, 16, 500)),
        s("adaptative, latence 100 ms", 1_500, 8, 100),
        s("adaptative, latence 100 ms", 1_500, 0, 100),
    ]
}

fn dense(sc: Scenario) -> Scenario {
    Scenario { dense: true, ..sc }
}

struct Outcome {
    sc: Scenario,
    urls: usize,
    secs: f64,
    rss_mb: f64,
    /// (règle, attendu, trouvé)
    accuracy: Vec<(&'static str, usize, usize)>,
}

/// Lance le vrai `callistseo crawl` et mesure sa durée et sa mémoire maximale (rusage du système).
fn run_child(
    cli: &Path,
    port: u16,
    sc: Scenario,
    out_dir: &Path,
) -> Result<(f64, f64, String), String> {
    let url = format!("http://127.0.0.1:{port}/p/0");
    let mut child = Command::new(cli)
        .args([
            "crawl",
            &url,
            "--allow-private",
            "--max-depth",
            "100",
            "--format",
            "csv",
        ])
        .args(["--max-pages", &(sc.pages + sc.pages / 40 + 10).to_string()])
        // 0 = adaptative : on ne fixe pas la concurrence, le crawler choisit (8 → 16).
        .args(if sc.concurrency == 0 {
            vec![]
        } else {
            vec!["--concurrency".to_owned(), sc.concurrency.to_string()]
        })
        .arg("--out")
        .arg(out_dir)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("impossible de lancer {} : {e}", cli.display()))?;
    let start = Instant::now();
    let mut stderr = String::new();
    child
        .stderr
        .take()
        .ok_or("stderr indisponible")?
        .read_to_string(&mut stderr)
        .map_err(|e| e.to_string())?;
    let (mut status, mut usage) = (0, unsafe { std::mem::zeroed::<libc::rusage>() });
    // `wait4` renvoie aussi la mémoire maximale (RSS) du processus fils.
    if unsafe { libc::wait4(child.id() as libc::pid_t, &mut status, 0, &mut usage) } < 0 {
        return Err("wait4 a échoué".into());
    }
    let secs = start.elapsed().as_secs_f64();
    let code = if libc::WIFEXITED(status) {
        libc::WEXITSTATUS(status)
    } else {
        -1
    };
    if code != 0 && code != 1 {
        return Err(format!(
            "le crawl s'est terminé avec le code {code} : {stderr}"
        ));
    }
    // ru_maxrss : octets sous macOS, kilo-octets ailleurs.
    let unit = if cfg!(target_os = "macos") {
        1.0
    } else {
        1024.0
    };
    Ok((secs, usage.ru_maxrss as f64 * unit / 1_048_576.0, stderr))
}

async fn run_scenario(cli: &Path, sc: Scenario, index: usize) -> Result<Outcome, String> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let server = tokio::spawn(site::serve(
        listener,
        sc.pages,
        Duration::from_millis(sc.delay_ms),
        if sc.dense {
            site::Corpus::DENSE
        } else {
            site::Corpus::LIGHT
        },
    ));
    let out_dir =
        std::env::temp_dir().join(format!("callistseo-bench-{}-{index}", std::process::id()));

    let (cli, dir) = (cli.to_owned(), out_dir.clone());
    // Appel bloquant : on le sort du runtime pour que le serveur continue de répondre.
    let child = tokio::task::spawn_blocking(move || run_child(&cli, port, sc, &dir))
        .await
        .map_err(|e| e.to_string())?;
    server.abort();
    let (secs, rss_mb, stderr) = child?;

    let urls = stderr
        .lines()
        .find(|l| l.contains(" URL,"))
        .and_then(|l| l.split_whitespace().next())
        .and_then(|n| n.parse().ok())
        .ok_or_else(|| format!("résumé du crawl introuvable : {stderr}"))?;
    let csv = std::fs::read_to_string(out_dir.join("issues.csv")).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_dir_all(&out_dir);
    let mut found: BTreeMap<&str, usize> = BTreeMap::new();
    for line in csv.lines().skip(1) {
        if let Some(rule) = line.split(',').nth(1) {
            *found.entry(rule).or_default() += 1;
        }
    }
    let accuracy = site::expected(sc.pages, sc.dense)
        .into_iter()
        .map(|(rule, want)| (rule, want, found.get(rule).copied().unwrap_or(0)))
        .collect();
    Ok(Outcome {
        sc,
        urls,
        secs,
        rss_mb,
        accuracy,
    })
}

fn percentile(sorted: &[Duration], p: f64) -> Duration {
    let rank = ((p * sorted.len() as f64).ceil() as usize).clamp(1, sorted.len());
    sorted[rank - 1]
}

/// Latence d'une requête (client HTTP du crawler, une à la fois, connexion persistante).
async fn latency(delay_ms: u64) -> Result<Vec<Duration>, String> {
    const WARMUP: usize = 20;
    const SAMPLES: usize = 300;
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let server = tokio::spawn(site::serve(
        listener,
        1_000,
        Duration::from_millis(delay_ms),
        site::Corpus::LIGHT,
    ));
    let client = callistseo_crawler::Client::new(&CrawlConfig {
        allow_private: true,
        ..CrawlConfig::default()
    })?;
    let mut samples = Vec::with_capacity(SAMPLES);
    for i in 0..WARMUP + SAMPLES {
        let url = url::Url::parse(&format!("http://127.0.0.1:{port}/p/{}", i % 1_000))
            .map_err(|e| e.to_string())?;
        let t = Instant::now();
        client.fetch(&url).await?;
        if i >= WARMUP {
            samples.push(t.elapsed());
        }
    }
    server.abort();
    samples.sort();
    Ok(samples)
}

fn sysctl(key: &str) -> Option<String> {
    let out = Command::new("sysctl").args(["-n", key]).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

fn proc_field(file: &str, key: &str) -> Option<String> {
    let text = std::fs::read_to_string(file).ok()?;
    let line = text.lines().find(|l| l.starts_with(key))?;
    Some(line.split_once(':')?.1.trim().to_owned())
}

fn machine() -> String {
    let cpu = sysctl("machdep.cpu.brand_string")
        .or_else(|| proc_field("/proc/cpuinfo", "model name"))
        .unwrap_or_else(|| "inconnu".into());
    let ram_gb = sysctl("hw.memsize")
        .and_then(|b| b.parse::<f64>().ok().map(|b| b / 1e9))
        .or_else(|| {
            proc_field("/proc/meminfo", "MemTotal").and_then(|v| {
                v.split_whitespace()
                    .next()
                    .and_then(|k| k.parse::<f64>().ok())
                    .map(|k| k / 1e6)
            })
        });
    let cores = std::thread::available_parallelism().map_or(0, |n| n.get());
    format!(
        "{} {} · {cpu} · {cores} cœurs logiques · {} Go de RAM",
        std::env::consts::OS,
        std::env::consts::ARCH,
        ram_gb.map_or("?".into(), |g| format!("{g:.0}"))
    )
}

fn report(
    machine: &str,
    outcomes: &[Outcome],
    lat: &[(u64, Vec<Duration>)],
    quick: bool,
) -> (String, bool) {
    let mut md = String::new();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let _ = writeln!(md, "# Benchmarks CallistSEO\n");
    let _ = writeln!(md, "- Date : {}", callistseo_export::iso_utc(now));
    let _ = writeln!(
        md,
        "- Version : {}{}",
        env!("CARGO_PKG_VERSION"),
        if quick { " (suite réduite)" } else { "" }
    );
    let _ = writeln!(md, "- Machine : {machine}");
    let _ = writeln!(md, "- Corpus : site synthétique déterministe servi en local (`callistseo-bench serve`), 4 à 5 liens par page, défauts injectés en nombre connu");
    let _ = writeln!(md, "- Méthode : le vrai binaire `callistseo crawl` (robots.txt respecté, profondeur 100, export CSV) tourne en processus séparé ; durée = du lancement à la fin ; mémoire = RSS maximale du processus (`wait4`). Le serveur du corpus tourne sur la même machine et lui prend un peu de CPU.");
    if cfg!(debug_assertions) {
        let _ = writeln!(md, "- ⚠️ Build de débogage : résultats non représentatifs (utiliser `cargo run --release -p callistseo-bench`).");
    }

    let _ = writeln!(md, "\n## Débit et mémoire\n");
    let _ = writeln!(md, "| Série | Corpus | Pages | Concurrence | Latence simulée | URL explorées | Durée | Débit | Mémoire max | Mémoire / 1 000 pages |");
    let _ = writeln!(
        md,
        "| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |"
    );
    for o in outcomes {
        let _ = writeln!(
            md,
            "| {} | {} | {} | {} | {} ms | {} | {:.2} s | {:.0} pages/s | {:.0} Mo | {:.1} Mo |",
            o.sc.group,
            if o.sc.dense { "dense" } else { "léger" },
            o.sc.pages,
            if o.sc.concurrency == 0 {
                "adaptative".to_owned()
            } else {
                o.sc.concurrency.to_string()
            },
            o.sc.delay_ms,
            o.urls,
            o.secs,
            o.urls as f64 / o.secs,
            o.rss_mb,
            o.rss_mb / (o.urls as f64 / 1000.0)
        );
    }

    let _ = writeln!(
        md,
        "\n## Latence d'une requête (client du crawler, une à la fois)\n"
    );
    let _ = writeln!(md, "| Latence simulée | p50 | p95 | p99 | max |");
    let _ = writeln!(md, "| ---: | ---: | ---: | ---: | ---: |");
    for (delay, s) in lat {
        let ms = |d: Duration| format!("{:.2} ms", d.as_secs_f64() * 1000.0);
        let _ = writeln!(
            md,
            "| {delay} ms | {} | {} | {} | {} |",
            ms(percentile(s, 0.5)),
            ms(percentile(s, 0.95)),
            ms(percentile(s, 0.99)),
            ms(*s.last().unwrap())
        );
    }

    let (mut exact, mut total) = (0, 0);
    let _ = writeln!(md, "\n## Exactitude des règles (vérité terrain exacte)\n");
    let _ = writeln!(
        md,
        "Chaque règle doit retrouver exactement le nombre de défauts injectés.\n"
    );
    let _ = writeln!(md, "| Pages | Règle | Attendu | Trouvé | |");
    let _ = writeln!(md, "| ---: | --- | ---: | ---: | --- |");
    for o in outcomes {
        for (rule, want, got) in &o.accuracy {
            total += 1;
            exact += usize::from(want == got);
            let _ = writeln!(
                md,
                "| {} | `{rule}` | {want} | {got} | {} |",
                o.sc.pages,
                if want == got { "✅" } else { "❌" }
            );
        }
    }
    let rate = if total == 0 {
        0.0
    } else {
        exact as f64 * 100.0 / total as f64
    };
    let _ = writeln!(md, "\n**{exact}/{total} comptes exacts ({rate:.1} %)** — objectif du plan : ≥ 99 % sur le corpus de test.");
    let _ = writeln!(md, "\nLes comptes de la règle `deep-page` ne sont pas comparés : elle dépend de la profondeur du parcours, pas d'un défaut injecté.");

    let _ = writeln!(md, "\n## Comment lire ces chiffres\n");
    let _ = writeln!(md, "- **Débit local ≠ débit réel.** Le corpus est servi en local, avec des pages de moins de 1 Ko : le crawler est limité par son propre CPU. Sur un vrai site, le débit est limité par la latence réseau et par la politesse envers le serveur ; les lignes « latence réseau » montrent ce régime (débit ≈ concurrence ÷ latence).");
    let _ = writeln!(md, "- **Mémoire.** La valeur inclut quelques Mo de base du processus : la pente entre les tailles (1 000 → 10 000 → 50 000 pages) est le coût réel par page. Le crawler garde tous les résultats en mémoire jusqu'à la fin de l'audit ; extrapoler à des sites très volumineux (plusieurs centaines de milliers d'URL) demande de vérifier la limite de RAM.");
    let _ = writeln!(md, "- **Concurrence élevée.** Par défaut il n'y a aucun délai entre requêtes : une concurrence de 64 est rapide en local mais agressive pour un vrai serveur. Utiliser `--delay-ms` ; le `Crawl-delay` de robots.txt est appliqué automatiquement.");
    let _ = writeln!(md, "- **Pas de comparaison avec d'autres outils** : ces mesures ne disent rien de leur performance relative. Le corpus peut être servi à n'importe quel crawler avec `callistseo-bench serve --pages N`.");
    let _ = writeln!(md, "\nReproduire : `cargo build --release -p callistseo-cli -p callistseo-bench && ./target/release/callistseo-bench run --out docs/benchmarks.md` (`--quick` pour une vérification rapide, code retour 1 si un compte de règle est inexact).");
    (md, exact == total)
}

#[tokio::main]
async fn main() -> ExitCode {
    match Cli::parse().command {
        Cmd::Serve {
            pages,
            port,
            delay_ms,
            dense,
        } => {
            let listener = match TcpListener::bind(("127.0.0.1", port)).await {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("impossible d'écouter sur le port {port} : {e}");
                    return ExitCode::from(2);
                }
            };
            eprintln!("site synthétique de {pages} pages sur http://127.0.0.1:{port}/p/0 (Ctrl-C pour arrêter)");
            let corpus = if dense {
                site::Corpus::DENSE
            } else {
                site::Corpus::LIGHT
            };
            site::serve(listener, pages, Duration::from_millis(delay_ms), corpus).await;
            ExitCode::SUCCESS
        }
        Cmd::Soak { runs, pages } => match soak(runs.max(2), pages).await {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("échec : {e}");
                ExitCode::from(1)
            }
        },
        Cmd::Run { cli, quick, out } => {
            let cli = cli.unwrap_or_else(|| {
                std::env::current_exe()
                    .ok()
                    .and_then(|p| p.parent().map(|d| d.join("callistseo")))
                    .unwrap_or_else(|| "callistseo".into())
            });
            if !cli.is_file() {
                eprintln!("binaire introuvable : {} (compilez-le avec `cargo build --release -p callistseo-cli` ou passez --cli)", cli.display());
                return ExitCode::from(2);
            }
            let mut outcomes = Vec::new();
            for (i, sc) in suite(quick).into_iter().enumerate() {
                eprintln!(
                    "[{}] {} pages, concurrence {}, latence {} ms…",
                    sc.group, sc.pages, sc.concurrency, sc.delay_ms
                );
                match run_scenario(&cli, sc, i).await {
                    Ok(o) => outcomes.push(o),
                    Err(e) => {
                        eprintln!("échec : {e}");
                        return ExitCode::from(2);
                    }
                }
            }
            let mut lat = Vec::new();
            for delay in [0, 20] {
                eprintln!("[latence] requêtes une à une, latence simulée {delay} ms…");
                match latency(delay).await {
                    Ok(s) => lat.push((delay, s)),
                    Err(e) => {
                        eprintln!("échec : {e}");
                        return ExitCode::from(2);
                    }
                }
            }
            let (md, all_exact) = report(&machine(), &outcomes, &lat, quick);
            match out {
                Some(path) => {
                    if let Some(dir) = path.parent() {
                        let _ = std::fs::create_dir_all(dir);
                    }
                    if let Err(e) = std::fs::write(&path, &md) {
                        eprintln!("écriture impossible : {e}");
                        return ExitCode::from(2);
                    }
                    eprintln!("rapport écrit dans {}", path.display());
                }
                None => println!("{md}"),
            }
            ExitCode::from(u8::from(!all_exact))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentiles() {
        let s: Vec<Duration> = (1..=100).map(Duration::from_millis).collect();
        assert_eq!(percentile(&s, 0.5), Duration::from_millis(50));
        assert_eq!(percentile(&s, 0.95), Duration::from_millis(95));
        assert_eq!(percentile(&s, 0.99), Duration::from_millis(99));
        assert_eq!(percentile(&s[..1], 0.99), Duration::from_millis(1));
    }

    #[test]
    fn suite_is_reproducible_and_quick_is_smaller() {
        assert_eq!(suite(false).len(), 15);
        assert!(suite(true).iter().all(|s| s.pages <= 500));
        assert!(suite(false).iter().any(|s| s.dense) && suite(true).iter().any(|s| s.dense));
    }
}
