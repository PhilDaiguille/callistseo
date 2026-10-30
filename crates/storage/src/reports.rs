//! Registre des rapports et lecture tabulaire des URL : tout ce que l'interface « poste de travail »
//! demande (compteurs par rapport, lignes triées/filtrées/paginées avec des colonnes au choix).
//!
//! Sécurité : le SQL est assemblé uniquement à partir de ce registre (prédicats, colonnes et tris
//! sont des constantes). Une valeur venant de l'extérieur n'est jamais insérée dans la requête :
//! elle est soit reconnue dans la liste blanche, soit passée en paramètre.

use super::{db, Result, Store};
use rusqlite::types::ValueRef;
use serde_json::Value;

pub struct Report {
    pub id: &'static str,
    pub group: &'static str,
    pub label: &'static str,
    /// Prédicat SQL sur la table `pages` aliasée `p`.
    pub predicate: &'static str,
    /// Colonnes affichées par défaut.
    pub columns: &'static [&'static str],
}

pub struct Column {
    pub id: &'static str,
    pub label: &'static str,
    /// Expression SQL sur `pages p`.
    pub sql: &'static str,
    pub numeric: bool,
}

const OK: &str = "p.status BETWEEN 200 AND 299";
// Approximation : le meta robots est exact ; pour l'en-tête on cherche une valeur qui commence par noindex.
const NOINDEX: &str = "(instr(lower(coalesce(p.robots,'')),'noindex')>0 OR instr(lower(coalesce(p.headers,'')),'\"x-robots-tag\",\"noindex')>0)";

const BASE: &[&str] = &["url", "status", "title", "depth"];
const RESPONSE: &[&str] = &[
    "url",
    "status",
    "ttfb_ms",
    "total_ms",
    "size",
    "content_type",
];
const META: &[&str] = &["url", "title", "description", "h1", "word_count"];

pub const REPORTS: &[Report] = &[
    Report { id: "all", group: "Ensemble du site", label: "Toutes les URL", predicate: "1=1", columns: BASE },
    Report { id: "html", group: "Ensemble du site", label: "Pages HTML", predicate: "p.is_html = 1", columns: BASE },
    Report { id: "internal-ok", group: "Réponse", label: "Succès (2xx)", predicate: "p.status BETWEEN 200 AND 299", columns: RESPONSE },
    Report { id: "redirects", group: "Réponse", label: "Redirections (3xx)", predicate: "p.status BETWEEN 300 AND 399", columns: &["url", "status", "redirect_to", "depth"] },
    Report { id: "client-errors", group: "Réponse", label: "Erreurs client (4xx)", predicate: "p.status BETWEEN 400 AND 499", columns: &["url", "status", "inlinks", "depth"] },
    Report { id: "server-errors", group: "Réponse", label: "Erreurs serveur (5xx)", predicate: "p.status >= 500", columns: &["url", "status", "inlinks", "depth"] },
    Report { id: "no-response", group: "Réponse", label: "Sans réponse", predicate: "p.status IS NULL AND p.error IS NOT NULL AND p.error <> 'bloqué par robots.txt'", columns: &["url", "error", "depth"] },
    Report { id: "robots-blocked", group: "Réponse", label: "Bloquées par robots.txt", predicate: "p.error = 'bloqué par robots.txt'", columns: &["url", "depth"] },
    Report { id: "slow", group: "Réponse", label: "Réponses lentes (> 1 s)", predicate: "p.ttfb_ms > 1000", columns: RESPONSE },
    Report { id: "indexable", group: "Indexation", label: "Indexables", predicate: "p.is_html = 1 AND p.status BETWEEN 200 AND 299 AND NOT-NOINDEX AND (p.canonical IS NULL OR p.canonical = p.url)", columns: BASE },
    Report { id: "non-indexable", group: "Indexation", label: "Non indexables (2xx)", predicate: "p.is_html = 1 AND p.status BETWEEN 200 AND 299 AND (NOINDEX OR (p.canonical IS NOT NULL AND p.canonical <> p.url))", columns: &["url", "robots", "canonical", "status"] },
    Report { id: "noindex", group: "Indexation", label: "Noindex", predicate: "NOINDEX", columns: &["url", "robots", "status"] },
    Report { id: "canonicalised", group: "Indexation", label: "Canonicalisées ailleurs", predicate: "p.canonical IS NOT NULL AND p.canonical <> p.url", columns: &["url", "canonical", "status"] },
    Report { id: "in-sitemap", group: "Indexation", label: "Dans le sitemap", predicate: "p.in_sitemap = 1", columns: BASE },
    Report { id: "not-in-sitemap", group: "Indexation", label: "Hors sitemap (indexables)", predicate: "p.in_sitemap = 0 AND p.is_html = 1 AND p.status BETWEEN 200 AND 299 AND NOT-NOINDEX", columns: BASE },
    Report { id: "hreflang", group: "Indexation", label: "Avec hreflang", predicate: "p.hreflang IS NOT NULL", columns: &["url", "lang", "hreflang_count", "status"] },
    Report { id: "title-missing", group: "Contenu", label: "Title absent ou vide", predicate: "p.is_html = 1 AND OK AND (p.title IS NULL OR p.title = '')", columns: META },
    Report { id: "title-duplicate", group: "Contenu", label: "Title dupliqué", predicate: "p.is_html = 1 AND OK AND p.title <> '' AND EXISTS (SELECT 1 FROM pages q WHERE q.run_id = p.run_id AND q.title = p.title AND q.is_html = 1 AND q.id <> p.id)", columns: &["url", "title", "status"] },
    Report { id: "description-missing", group: "Contenu", label: "Description absente ou vide", predicate: "p.is_html = 1 AND OK AND (p.description IS NULL OR p.description = '')", columns: META },
    Report { id: "h1-missing", group: "Contenu", label: "H1 absent", predicate: "p.is_html = 1 AND OK AND p.h1 = '[]'", columns: META },
    Report { id: "thin", group: "Contenu", label: "Peu de contenu (< 200 mots)", predicate: "p.is_html = 1 AND OK AND p.word_count < 200", columns: META },
    Report { id: "content-duplicate", group: "Contenu", label: "Contenu dupliqué", predicate: "p.is_html = 1 AND OK AND p.word_count >= 200 AND EXISTS (SELECT 1 FROM pages q WHERE q.run_id = p.run_id AND q.content_hash = p.content_hash AND q.is_html = 1 AND q.id <> p.id)", columns: &["url", "title", "word_count", "content_hash"] },
    Report { id: "structured-data", group: "Contenu", label: "Avec JSON-LD", predicate: "p.json_ld IS NOT NULL", columns: &["url", "title", "status"] },
];

pub const COLUMNS: &[Column] = &[
    Column {
        id: "url",
        label: "URL",
        sql: "p.url",
        numeric: false,
    },
    Column {
        id: "status",
        label: "Statut",
        sql: "p.status",
        numeric: true,
    },
    Column {
        id: "depth",
        label: "Profondeur",
        sql: "p.depth",
        numeric: true,
    },
    Column {
        id: "title",
        label: "Title",
        sql: "p.title",
        numeric: false,
    },
    Column {
        id: "description",
        label: "Description",
        sql: "p.description",
        numeric: false,
    },
    Column {
        id: "h1",
        label: "H1",
        sql: "json_extract(p.h1, '$[0]')",
        numeric: false,
    },
    Column {
        id: "h1_count",
        label: "Nb H1",
        sql: "json_array_length(p.h1)",
        numeric: true,
    },
    Column {
        id: "canonical",
        label: "Canonical",
        sql: "p.canonical",
        numeric: false,
    },
    Column {
        id: "robots",
        label: "Meta robots",
        sql: "p.robots",
        numeric: false,
    },
    Column {
        id: "redirect_to",
        label: "Redirection",
        sql: "p.redirect_to",
        numeric: false,
    },
    Column {
        id: "error",
        label: "Erreur",
        sql: "p.error",
        numeric: false,
    },
    Column {
        id: "in_sitemap",
        label: "Sitemap",
        sql: "p.in_sitemap",
        numeric: true,
    },
    Column {
        id: "ttfb_ms",
        label: "1er octet (ms)",
        sql: "p.ttfb_ms",
        numeric: true,
    },
    Column {
        id: "total_ms",
        label: "Total (ms)",
        sql: "p.total_ms",
        numeric: true,
    },
    Column {
        id: "size",
        label: "Taille (o)",
        sql: "p.size",
        numeric: true,
    },
    Column {
        id: "content_type",
        label: "Type",
        sql: "p.content_type",
        numeric: false,
    },
    Column {
        id: "word_count",
        label: "Mots",
        sql: "p.word_count",
        numeric: true,
    },
    Column {
        id: "lang",
        label: "Langue",
        sql: "p.lang",
        numeric: false,
    },
    Column {
        id: "content_hash",
        label: "Empreinte",
        sql: "p.content_hash",
        numeric: false,
    },
    Column {
        id: "images_total",
        label: "Images",
        sql: "p.images_total",
        numeric: true,
    },
    Column {
        id: "nofollow_links",
        label: "Liens nofollow",
        sql: "p.nofollow_links",
        numeric: true,
    },
    Column {
        id: "hreflang_count",
        label: "Nb hreflang",
        sql: "coalesce(json_array_length(p.hreflang), 0)",
        numeric: true,
    },
    Column {
        id: "extracted",
        label: "Extraction",
        sql: "p.extracted",
        numeric: false,
    },
    Column {
        id: "inlinks",
        label: "Liens entrants",
        sql: "p.inlinks",
        numeric: true,
    },
    Column {
        id: "outlinks",
        label: "Liens sortants internes",
        sql: "p.outlinks",
        numeric: true,
    },
    Column {
        id: "title_tags",
        label: "Nb title",
        sql: "p.title_tags",
        numeric: true,
    },
    Column {
        id: "canonical_tags",
        label: "Nb canonical",
        sql: "p.canonical_tags",
        numeric: true,
    },
    Column {
        id: "images_unsized",
        label: "Images sans dimensions",
        sql: "p.images_unsized",
        numeric: true,
    },
    Column {
        id: "social",
        label: "Partage social",
        sql: "(SELECT group_concat(value, ', ') FROM json_each(p.social))",
        numeric: false,
    },
    Column {
        id: "icon",
        label: "Icône déclarée",
        sql: "CASE p.icon WHEN 1 THEN 'oui' WHEN 0 THEN 'non' END",
        numeric: false,
    },
    Column {
        id: "blocked_resources",
        label: "CSS/JS bloqués (Googlebot)",
        sql: "coalesce(json_array_length(p.blocked_resources), 0)",
        numeric: true,
    },
    Column {
        id: "issues",
        label: "Problèmes",
        sql: "(SELECT COUNT(*) FROM issues i WHERE i.run_id = p.run_id AND i.url = p.url)",
        numeric: true,
    },
];

pub fn report(id: &str) -> Option<&'static Report> {
    REPORTS.iter().find(|r| r.id == id)
}

pub fn column(id: &str) -> Option<&'static Column> {
    COLUMNS.iter().find(|c| c.id == id)
}

/// Prédicat final : les alias `OK` / `NOINDEX` / `NOT-NOINDEX` sont développés ici, une seule fois.
fn predicate(r: &Report) -> String {
    r.predicate
        .replace("NOT-NOINDEX", &format!("NOT {NOINDEX}"))
        .replace("NOINDEX", NOINDEX)
        .replace(" OK ", &format!(" ({OK}) "))
}

/// Périmètre d'une requête de lignes : un rapport du registre, ou les URL touchées par une règle.
pub enum Scope<'a> {
    Report(&'static Report),
    Rule(&'a str),
}

/// Segment : un filtre enregistré, combinable avec n'importe quel rapport (tous les critères
/// renseignés doivent être vrais). Chaque critère est une fraction de SQL constante ; les valeurs
/// de l'utilisateur sont toujours des paramètres.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SegmentDef {
    /// L'URL contient ce texte (sans tenir compte de la casse).
    pub url_contains: Option<String>,
    /// L'URL commence par ce préfixe (ex. `https://site.fr/blog/`).
    pub url_prefix: Option<String>,
    /// Classe de statut HTTP : 2, 3, 4 ou 5.
    pub status_class: Option<u8>,
    pub max_depth: Option<i64>,
    /// L'URL a au moins un problème de cette règle.
    pub issue_rule: Option<String>,
    /// Un extracteur personnalisé a trouvé au moins une valeur.
    pub has_extracted: Option<String>,
}

impl SegmentDef {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Fraction SQL (avec ` AND ` initial) et paramètres ; `first` est le numéro du premier `?`.
    fn sql(&self, first: usize) -> (String, Vec<rusqlite::types::Value>) {
        use rusqlite::types::Value as V;
        let (mut sql, mut args) = (String::new(), Vec::new());
        let mut add = |fragment: &str, v: V| {
            args.push(v);
            sql.push_str(&fragment.replace('#', &(first + args.len() - 1).to_string()));
        };
        if let Some(t) = &self.url_contains {
            add(" AND instr(lower(p.url), lower(?#)) > 0", t.clone().into());
        }
        if let Some(t) = &self.url_prefix {
            add(" AND substr(p.url, 1, length(?#)) = ?#", t.clone().into());
        }
        if let Some(c) = self.status_class {
            add(
                " AND p.status >= ?# * 100 AND p.status < (?# + 1) * 100",
                i64::from(c).into(),
            );
        }
        if let Some(d) = self.max_depth {
            add(" AND p.depth <= ?#", d.into());
        }
        if let Some(r) = &self.issue_rule {
            add(" AND EXISTS (SELECT 1 FROM issues si WHERE si.run_id = p.run_id AND si.url = p.url AND si.rule = ?#)", r.clone().into());
        }
        if let Some(n) = &self.has_extracted {
            add(
                " AND json_extract(p.extracted, '$.' || ?#) IS NOT NULL",
                n.clone().into(),
            );
        }
        (sql, args)
    }

    /// Valide les valeurs (les identifiants et plafonds ; les motifs libres restent des paramètres).
    pub fn validate(&self) -> std::result::Result<(), String> {
        let text_ok =
            |t: &Option<String>| t.as_ref().is_none_or(|v| !v.is_empty() && v.len() <= 500);
        if !text_ok(&self.url_contains) || !text_ok(&self.url_prefix) {
            return Err("segment : texte de 1 à 500 caractères".into());
        }
        if self.status_class.is_some_and(|c| !(2..=5).contains(&c)) {
            return Err("segment : classe de statut 2, 3, 4 ou 5".into());
        }
        if self.max_depth.is_some_and(|d| !(0..=1000).contains(&d)) {
            return Err("segment : profondeur entre 0 et 1000".into());
        }
        if self.issue_rule.as_ref().is_some_and(|r| {
            r.len() > 60 || !r.bytes().all(|b| b.is_ascii_lowercase() || b == b'-')
        }) {
            return Err("segment : identifiant de règle invalide".into());
        }
        if self.has_extracted.as_ref().is_some_and(|n| {
            !(1..=30).contains(&n.len())
                || !n.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        }) {
            return Err("segment : nom d'extracteur invalide".into());
        }
        Ok(())
    }
}

pub struct RowQuery<'a> {
    pub scope: Scope<'a>,
    pub sort: Option<&'static Column>,
    pub desc: bool,
    pub columns: Vec<&'static Column>,
    pub search: Option<&'a str>,
    pub segment: Option<&'a SegmentDef>,
    pub limit: i64,
    pub offset: i64,
}

fn json_value(v: ValueRef) -> Value {
    match v {
        ValueRef::Integer(n) => n.into(),
        ValueRef::Real(f) => f.into(),
        ValueRef::Text(t) => String::from_utf8_lossy(t).into_owned().into(),
        _ => Value::Null,
    }
}

impl Store {
    /// Nombre d'URL de chaque rapport, en un seul parcours de la table.
    pub fn report_counts(&self, run_id: i64) -> Result<Vec<(&'static str, i64)>> {
        let sums: Vec<String> = REPORTS
            .iter()
            .map(|r| {
                format!(
                    "COALESCE(SUM(CASE WHEN {} THEN 1 ELSE 0 END), 0)",
                    predicate(r)
                )
            })
            .collect();
        let sql = format!(
            "SELECT {} FROM pages p WHERE p.run_id = ?1",
            sums.join(", ")
        );
        self.conn
            .query_row(&sql, [run_id], |row| {
                REPORTS
                    .iter()
                    .enumerate()
                    .map(|(i, r)| Ok((r.id, row.get::<_, i64>(i)?)))
                    .collect()
            })
            .map_err(db)
    }

    /// Lignes d'un rapport : (total avant pagination, une ligne par URL avec les colonnes demandées).
    pub fn report_rows(&self, run_id: i64, q: &RowQuery) -> Result<(i64, Vec<Vec<Value>>)> {
        let (pred, rule) = match q.scope {
            Scope::Report(r) => (predicate(r), None),
            Scope::Rule(rule) => (
                "EXISTS (SELECT 1 FROM issues i WHERE i.run_id = p.run_id AND i.url = p.url AND i.rule = ?3)".to_owned(),
                Some(rule),
            ),
        };
        let (seg_sql, seg_args) = q.segment.map_or_else(Default::default, |d| d.sql(4));
        let filter = format!(
            "p.run_id = ?1 AND ({pred})
             AND (?2 IS NULL OR instr(lower(p.url), lower(?2)) > 0
                  OR instr(lower(coalesce(p.title, '')), lower(?2)) > 0)
             AND (?3 IS ?3){seg_sql}"
        );
        use rusqlite::types::Value as V;
        let mut args: Vec<V> = vec![
            run_id.into(),
            q.search.map(str::to_owned).into(),
            rule.map(str::to_owned).into(),
        ];
        args.extend(seg_args);
        let (lim, off) = (args.len() + 1, args.len() + 2);
        let total: i64 = self
            .conn
            .query_row(
                &format!("SELECT COUNT(*) FROM pages p WHERE {filter}"),
                rusqlite::params_from_iter(args.iter()),
                |r| r.get(0),
            )
            .map_err(db)?;
        let select = q
            .columns
            .iter()
            .map(|c| c.sql)
            .collect::<Vec<_>>()
            .join(", ");
        let order = match q.sort {
            Some(c) => format!(
                "{} IS NULL, {} {}, p.id",
                c.sql,
                c.sql,
                if q.desc { "DESC" } else { "ASC" }
            ),
            None => "p.id".to_owned(),
        };
        let mut stmt = self
            .conn
            .prepare(&format!(
                "SELECT {select} FROM pages p WHERE {filter} ORDER BY {order} LIMIT ?{lim} OFFSET ?{off}"
            ))
            .map_err(db)?;
        let n = q.columns.len();
        let rows = stmt
            .query_map(
                rusqlite::params_from_iter(
                    args.iter().chain([&V::from(q.limit), &V::from(q.offset)]),
                ),
                |r| (0..n).map(|i| Ok(json_value(r.get_ref(i)?))).collect(),
            )
            .map_err(db)?;
        Ok((
            total,
            rows.collect::<std::result::Result<_, _>>().map_err(db)?,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn registry_is_consistent() {
        let ids: HashSet<_> = REPORTS.iter().map(|r| r.id).collect();
        assert_eq!(ids.len(), REPORTS.len());
        let cols: HashSet<_> = COLUMNS.iter().map(|c| c.id).collect();
        assert_eq!(cols.len(), COLUMNS.len());
        for r in REPORTS {
            assert!(r.columns.iter().all(|c| column(c).is_some()), "{}", r.id);
            assert!(
                !predicate(r).contains("OK")
                    || r.predicate.contains("OK") && !predicate(r).contains(" OK ")
            );
        }
        assert!(report("inconnu").is_none() && column("p.url; DROP TABLE pages").is_none());
    }

    use callistseo_core::{CrawlConfig, CrawledPage, Issue, PageFacts, PageMeta, Severity};
    use std::sync::Arc;
    use url::Url;

    fn u(p: &str) -> Url {
        Url::parse(&format!("http://a.test{p}")).unwrap()
    }

    fn page(
        path: &str,
        status: u16,
        title: Option<&str>,
        words: u32,
        links: &[&str],
    ) -> CrawledPage {
        CrawledPage {
            facts: PageFacts {
                word_count: Some(words),
                content_hash: Some(if path == "/slow" { "z" } else { "h" }.into()),
                ttfb_ms: Some(if path == "/slow" { 2000 } else { 50 }),
                ..PageFacts::default()
            },
            in_sitemap: false,
            is_html: status < 300,
            meta: PageMeta {
                title: title.map(str::to_owned),
                h1: vec!["Un titre".into()],
                ..PageMeta::default()
            },
            url: u(path),
            depth: 1,
            status: Some(status),
            redirect_to: None,
            links: links.iter().map(|l| Arc::new(u(l))).collect(),
            error: None,
        }
    }

    fn store() -> (Store, i64) {
        let mut a = page("/a", 200, Some("Dup"), 300, &["/b", "/gone"]);
        a.in_sitemap = true;
        let mut noindex = page("/n", 200, Some("Seul"), 10, &[]);
        noindex.meta.robots = Some("noindex, follow".into());
        let pages = vec![
            a,
            page("/b", 200, Some("Dup"), 300, &["/a"]),
            page("/gone", 404, None, 0, &[]),
            noindex,
            page("/slow", 200, Some("Lent"), 500, &[]),
        ];
        let issue = Issue {
            rule: "http-error",
            severity: Severity::Error,
            url: u("/gone"),
            message: "m".into(),
        };
        let mut store = Store::open_in_memory().unwrap();
        let id = store
            .save_run("p", &u("/"), &CrawlConfig::default(), &pages, &[issue])
            .unwrap();
        (store, id)
    }

    fn query<'a>(scope: Scope<'a>, cols: &[&str]) -> RowQuery<'a> {
        RowQuery {
            scope,
            sort: None,
            desc: false,
            columns: cols.iter().map(|c| column(c).unwrap()).collect(),
            search: None,
            segment: None,
            limit: 50,
            offset: 0,
        }
    }

    fn urls(store: &Store, run: i64, report_id: &str) -> Vec<String> {
        let q = query(Scope::Report(report(report_id).unwrap()), &["url"]);
        let (total, rows) = store.report_rows(run, &q).unwrap();
        assert_eq!(total as usize, rows.len());
        rows.into_iter()
            .map(|r| r[0].as_str().unwrap().replace("http://a.test", ""))
            .collect()
    }

    #[test]
    fn every_report_runs_and_counts_match_rows() {
        let (store, run) = store();
        let counts = store.report_counts(run).unwrap();
        assert_eq!(counts.len(), REPORTS.len());
        for (id, n) in counts {
            assert_eq!(urls(&store, run, id).len() as i64, n, "rapport {id}");
        }
    }

    #[test]
    fn reports_select_the_right_urls() {
        let (store, run) = store();
        assert_eq!(urls(&store, run, "all").len(), 5);
        assert_eq!(urls(&store, run, "client-errors"), ["/gone"]);
        assert_eq!(urls(&store, run, "noindex"), ["/n"]);
        assert_eq!(urls(&store, run, "indexable"), ["/a", "/b", "/slow"]);
        assert_eq!(urls(&store, run, "title-duplicate"), ["/a", "/b"]);
        assert_eq!(urls(&store, run, "content-duplicate"), ["/a", "/b"]);
        assert_eq!(urls(&store, run, "title-missing"), Vec::<String>::new()); // /gone n'est pas du HTML 2xx
        assert_eq!(urls(&store, run, "thin"), ["/n"]);
        assert_eq!(urls(&store, run, "slow"), ["/slow"]);
        assert_eq!(urls(&store, run, "in-sitemap"), ["/a"]);
        assert_eq!(urls(&store, run, "not-in-sitemap"), ["/b", "/slow"]);
    }

    /// Chaque colonne de la liste blanche est une expression SQL valide, affichée comme triée.
    #[test]
    fn every_column_can_be_shown_and_sorted() {
        let (store, run) = store();
        let ids: Vec<&str> = COLUMNS.iter().map(|c| c.id).collect();
        for c in COLUMNS {
            let mut q = query(Scope::Report(report("all").unwrap()), &ids);
            q.sort = Some(c);
            let (total, rows) = store
                .report_rows(run, &q)
                .unwrap_or_else(|e| panic!("colonne {} : {e}", c.id));
            assert_eq!((total, rows[0].len()), (5, ids.len()), "colonne {}", c.id);
        }
    }

    #[test]
    fn rows_support_columns_sort_search_paging_and_rules() {
        let (store, run) = store();
        let mut q = query(
            Scope::Report(report("html").unwrap()),
            &["url", "inlinks", "issues", "word_count"],
        );
        q.sort = column("word_count");
        q.desc = true;
        let (total, rows) = store.report_rows(run, &q).unwrap();
        assert_eq!(total, 4);
        assert_eq!(rows[0][0], "http://a.test/slow");
        assert_eq!(rows[0][3], 500);
        // /a reçoit un lien de /b, /b un lien de /a ; /gone a une erreur.
        let by_url = |p: &str| {
            rows.iter()
                .find(|r| r[0] == format!("http://a.test{p}"))
                .unwrap()
                .clone()
        };
        assert_eq!(
            (by_url("/a")[1].clone(), by_url("/b")[1].clone()),
            (1.into(), 1.into())
        );

        q.limit = 2;
        q.offset = 3;
        assert_eq!(store.report_rows(run, &q).unwrap().1.len(), 1);

        let mut q = query(Scope::Report(report("all").unwrap()), &["url", "issues"]);
        q.search = Some("GONE");
        let (total, rows) = store.report_rows(run, &q).unwrap();
        assert_eq!((total, rows[0][1].clone()), (1, 1.into()));

        let q = query(Scope::Rule("http-error"), &["url"]);
        assert_eq!(
            store.report_rows(run, &q).unwrap().1,
            [[Value::from("http://a.test/gone")]]
        );
        let q = query(Scope::Rule("x' OR '1'='1"), &["url"]);
        assert_eq!(
            store.report_rows(run, &q).unwrap().0,
            0,
            "valeur passée en paramètre, jamais en SQL"
        );
    }

    fn count(store: &Store, run: i64, def: SegmentDef) -> i64 {
        def.validate().unwrap();
        store.segment_count(run, &def).unwrap()
    }

    #[test]
    fn segments_filter_by_each_criterion_and_combine() {
        let mut p = page("/blog/x", 200, Some("T"), 300, &[]);
        p.facts.extracted = vec![("ga".into(), vec!["UA-1".into()])];
        let mut q = page("/shop/y", 404, None, 0, &[]);
        q.depth = 3;
        let pages = vec![p, q, page("/blog/z", 200, Some("T2"), 300, &[])];
        let issue = Issue {
            rule: "http-error",
            severity: Severity::Error,
            url: u("/shop/y"),
            message: "m".into(),
        };
        let mut store = Store::open_in_memory().unwrap();
        let run = store
            .save_run("p", &u("/"), &CrawlConfig::default(), &pages, &[issue])
            .unwrap();
        let seg = |f: fn(&mut SegmentDef)| {
            let mut d = SegmentDef::default();
            f(&mut d);
            count(&store, run, d)
        };
        assert_eq!(seg(|_| {}), 3, "un segment vide garde tout");
        assert_eq!(seg(|d| d.url_contains = Some("BLOG".into())), 2);
        assert_eq!(
            seg(|d| d.url_prefix = Some("http://a.test/shop/".into())),
            1
        );
        assert_eq!(seg(|d| d.status_class = Some(4)), 1);
        assert_eq!(seg(|d| d.status_class = Some(2)), 2);
        assert_eq!(seg(|d| d.max_depth = Some(1)), 2);
        assert_eq!(seg(|d| d.issue_rule = Some("http-error".into())), 1);
        assert_eq!(seg(|d| d.has_extracted = Some("ga".into())), 1);
        assert_eq!(seg(|d| d.has_extracted = Some("autre".into())), 0);
        assert_eq!(
            seg(|d| {
                d.url_contains = Some("blog".into());
                d.status_class = Some(2);
                d.has_extracted = Some("ga".into())
            }),
            1
        );
        // Une valeur hostile reste une valeur : aucune correspondance, aucune erreur SQL.
        assert_eq!(seg(|d| d.url_contains = Some("x' OR '1'='1".into())), 0);

        // Le segment se combine avec un rapport.
        let mut query = query(
            Scope::Report(report("html").unwrap()),
            &["url", "extracted"],
        );
        let def = SegmentDef {
            url_contains: Some("blog".into()),
            ..Default::default()
        };
        query.segment = Some(&def);
        let (total, rows) = store.report_rows(run, &query).unwrap();
        assert_eq!(total, 2);
        assert_eq!(rows[0][1], r#"{"ga":["UA-1"]}"#);
    }

    #[test]
    fn segment_definitions_are_validated() {
        let bad = |f: fn(&mut SegmentDef)| {
            let mut d = SegmentDef::default();
            f(&mut d);
            assert!(d.validate().is_err());
        };
        bad(|d| d.status_class = Some(7));
        bad(|d| d.max_depth = Some(-1));
        bad(|d| d.url_contains = Some(String::new()));
        bad(|d| d.issue_rule = Some("x; DROP".into()));
        bad(|d| d.has_extracted = Some("a b".into()));
        assert!(serde_json::from_str::<SegmentDef>(r#"{"inconnu":1}"#).is_err());
    }

    #[test]
    fn segments_are_stored_per_project_with_unique_names() {
        let mut store = Store::open_in_memory().unwrap();
        let p = store.create_project("a", &u("/")).unwrap();
        let def = SegmentDef {
            url_contains: Some("blog".into()),
            ..Default::default()
        };
        let id = store.create_segment(p, "Blog", &def).unwrap();
        assert!(store
            .create_segment(p, "Blog", &def)
            .unwrap_err()
            .contains("existe déjà"));
        let list = store.segments(p).unwrap();
        assert_eq!((list.len(), list[0].0, list[0].1.as_str()), (1, id, "Blog"));
        assert_eq!(store.segment(id).unwrap().unwrap().2, def);
        for i in 0..19 {
            store.create_segment(p, &format!("s{i}"), &def).unwrap();
        }
        assert!(store.create_segment(p, "trop", &def).is_err(), "20 au plus");
        assert!(store.delete_segment(id).unwrap() && !store.delete_segment(id).unwrap());
    }
}
