//! Persistance SQLite : projets, exécutions de crawl, pages, liens et problèmes.
//! Chaque exécution est nouvelle ; les anciennes ne sont jamais écrasées.

use callistseo_core::{
    CrawlConfig, CrawledPage, Issue, PageFacts, PageMeta, Severity, UrlInterner,
};
use rusqlite::{params, Connection, OptionalExtension, Row, Transaction};
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use url::Url;

pub type Result<T> = std::result::Result<T, String>;

fn db(e: rusqlite::Error) -> String {
    e.to_string()
}

/// Migrations versionnées via `PRAGMA user_version` : on n'ajoute qu'à la fin de la liste.
const MIGRATIONS: &[&str] = &[
    "
CREATE TABLE projects (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    start_url TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE TABLE crawl_runs (
    id INTEGER PRIMARY KEY,
    project_id INTEGER NOT NULL REFERENCES projects(id),
    started_at INTEGER NOT NULL,
    settings TEXT NOT NULL
);
CREATE TABLE pages (
    id INTEGER PRIMARY KEY,
    run_id INTEGER NOT NULL REFERENCES crawl_runs(id) ON DELETE CASCADE,
    url TEXT NOT NULL,
    depth INTEGER NOT NULL,
    status INTEGER,
    redirect_to TEXT,
    is_html INTEGER NOT NULL,
    in_sitemap INTEGER NOT NULL,
    title TEXT,
    description TEXT,
    h1 TEXT NOT NULL,
    canonical TEXT,
    robots TEXT,
    error TEXT
);
CREATE TABLE links (
    page_id INTEGER NOT NULL REFERENCES pages(id) ON DELETE CASCADE,
    target TEXT NOT NULL
);
CREATE TABLE issues (
    id INTEGER PRIMARY KEY,
    run_id INTEGER NOT NULL REFERENCES crawl_runs(id) ON DELETE CASCADE,
    rule TEXT NOT NULL,
    severity TEXT NOT NULL,
    url TEXT NOT NULL,
    message TEXT NOT NULL
);
CREATE INDEX idx_pages_run_url ON pages(run_id, url);
CREATE INDEX idx_links_page ON links(page_id);
CREATE INDEX idx_issues_run_rule ON issues(run_id, rule);
CREATE INDEX idx_issues_run_severity ON issues(run_id, severity);
",
    // v2 : état d'une exécution (les exécutions existantes sont terminées).
    "
ALTER TABLE crawl_runs ADD COLUMN status TEXT NOT NULL DEFAULT 'done';
ALTER TABLE crawl_runs ADD COLUMN finished_at INTEGER;
ALTER TABLE crawl_runs ADD COLUMN error TEXT;
",
    // v3 : faits mesurés par URL (réponse et contenu). Les audits existants gardent NULL.
    // Les colonnes scalaires servent aux tris et filtres SQL ; les listes sont du JSON.
    "
ALTER TABLE pages ADD COLUMN ttfb_ms INTEGER;
ALTER TABLE pages ADD COLUMN total_ms INTEGER;
ALTER TABLE pages ADD COLUMN size INTEGER;
ALTER TABLE pages ADD COLUMN content_type TEXT;
ALTER TABLE pages ADD COLUMN headers TEXT;
ALTER TABLE pages ADD COLUMN content_hash TEXT;
ALTER TABLE pages ADD COLUMN word_count INTEGER;
ALTER TABLE pages ADD COLUMN lang TEXT;
ALTER TABLE pages ADD COLUMN viewport TEXT;
ALTER TABLE pages ADD COLUMN h2 TEXT;
ALTER TABLE pages ADD COLUMN hreflang TEXT;
ALTER TABLE pages ADD COLUMN images TEXT;
ALTER TABLE pages ADD COLUMN images_total INTEGER;
ALTER TABLE pages ADD COLUMN json_ld TEXT;
ALTER TABLE pages ADD COLUMN nofollow_links INTEGER;
CREATE INDEX idx_pages_run_hash ON pages(run_id, content_hash);
",
    // v4 : index pour les rapports (doublons de title, liens entrants, problèmes par URL).
    "
CREATE INDEX idx_pages_run_title ON pages(run_id, title);
CREATE INDEX idx_links_target ON links(target);
CREATE INDEX idx_issues_run_url ON issues(run_id, url);
",
    // v5 : configuration d'audit par projet (JSON validé par l'API ; aucun secret ici).
    "
ALTER TABLE projects ADD COLUMN config TEXT;
",
    // v6 : un audit récurrent au plus par projet.
    "
CREATE TABLE schedules (
    project_id INTEGER PRIMARY KEY REFERENCES projects(id) ON DELETE CASCADE,
    interval_minutes INTEGER NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    next_run_at INTEGER NOT NULL,
    last_run_id INTEGER
);
",
    // v7 : extraction personnalisée (JSON {nom: [valeurs]}) et segments enregistrés par projet.
    "
ALTER TABLE pages ADD COLUMN extracted TEXT;
CREATE TABLE segments (
    id INTEGER PRIMARY KEY,
    project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    definition TEXT NOT NULL,
    UNIQUE (project_id, name)
);
",
    // v8 : liens internes précalculés par page (entrants : pages distinctes crawlées qui y mènent ;
    // sortants : pages crawlées distinctes visées). Les tris, le maillage et les rapports n'ont plus
    // à joindre la table `links` à chaque requête. Les audits existants sont recalculés ici.
    "
ALTER TABLE pages ADD COLUMN inlinks INTEGER;
ALTER TABLE pages ADD COLUMN outlinks INTEGER;
UPDATE pages SET
    inlinks = (SELECT COUNT(*) FROM links l JOIN pages s ON s.id = l.page_id
               WHERE l.target = pages.url AND s.run_id = pages.run_id AND s.id != pages.id),
    outlinks = (SELECT COUNT(*) FROM links l JOIN pages t ON t.run_id = pages.run_id AND t.url = l.target
                WHERE l.page_id = pages.id AND t.id != pages.id);
CREATE INDEX idx_pages_run_inlinks ON pages(run_id, inlinks);
",
    // v9 : simhash du texte visible (quasi-doublons de contenu).
    "
ALTER TABLE pages ADD COLUMN simhash TEXT;
",
    // v10 : AMP et microdata.
    "
ALTER TABLE pages ADD COLUMN amp_html TEXT;
ALTER TABLE pages ADD COLUMN is_amp INTEGER;
ALTER TABLE pages ADD COLUMN microdata INTEGER;
ALTER TABLE pages ADD COLUMN microdata_untyped INTEGER;
",
    // v11 : `<lastmod>` du sitemap.
    "
ALTER TABLE pages ADD COLUMN sitemap_lastmod TEXT;
",
    // v12 : ancres vides / génériques et liens externes cassés.
    "
ALTER TABLE pages ADD COLUMN anchor_empty INTEGER;
ALTER TABLE pages ADD COLUMN anchor_generic INTEGER;
ALTER TABLE pages ADD COLUMN external_broken TEXT;
",
    // v13 : robots IA interdits par robots.txt (URL de départ).
    "
ALTER TABLE pages ADD COLUMN ai_bots_blocked TEXT;
",
];

/// Nœud du graphe de liens : (URL, profondeur, statut HTTP).
pub type GraphNode = (String, i64, Option<u16>);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Schedule {
    pub project_id: i64,
    pub interval_minutes: i64,
    pub enabled: bool,
    pub next_run_at: i64,
    pub last_run_id: Option<i64>,
}

fn read_schedule(r: &Row) -> rusqlite::Result<Schedule> {
    Ok(Schedule {
        project_id: r.get(0)?,
        interval_minutes: r.get(1)?,
        enabled: r.get(2)?,
        next_run_at: r.get(3)?,
        last_run_id: r.get(4)?,
    })
}

mod reports;
pub use reports::{column, report, Column, Report, RowQuery, Scope, SegmentDef, COLUMNS, REPORTS};

pub struct Store {
    conn: Connection,
}

#[derive(Debug, Clone)]
pub struct Project {
    pub id: i64,
    pub name: String,
    pub start_url: String,
    /// Secondes Unix.
    pub created_at: i64,
}

#[derive(Debug, Clone)]
pub struct RunInfo {
    pub id: i64,
    pub project_id: i64,
    pub project: String,
    pub start_url: String,
    /// `running`, `done`, `failed` ou `cancelled`.
    pub status: String,
    /// Secondes Unix.
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub error: Option<String>,
    pub pages: i64,
    pub errors: i64,
    pub warnings: i64,
}

#[derive(Debug, Clone, Default)]
pub struct LinkStats {
    /// (profondeur, nombre de pages)
    pub depths: Vec<(i64, i64)>,
    /// Liens entre deux pages crawlées différentes.
    pub internal_links: i64,
    /// Pages les plus liées : (URL, pages qui y mènent).
    pub top_linked: Vec<(String, i64)>,
    /// Pages qui lient vers le plus de pages crawlées : (URL, pages ciblées).
    pub top_linking: Vec<(String, i64)>,
    /// Pages (hors accueil) vers lesquelles aucune autre page crawlée ne lie.
    pub orphans_total: i64,
    pub orphans: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct PageQuery {
    /// Sous-chaîne (insensible à la casse) de l'URL ou du title.
    pub search: Option<String>,
    pub status: Option<u16>,
    pub limit: i64,
    pub offset: i64,
}

#[derive(Debug, Clone, Default)]
pub struct IssueQuery {
    pub severity: Option<String>,
    pub rule: Option<String>,
    pub limit: i64,
    pub offset: i64,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        Self::init(Connection::open(path).map_err(db)?)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory().map_err(db)?)
    }

    fn init(mut conn: Connection) -> Result<Self> {
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(db)?;
        conn.pragma_update(None, "foreign_keys", true).map_err(db)?;
        let version: i64 = conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .map_err(db)?;
        let version = version as usize;
        if version > MIGRATIONS.len() {
            return Err(format!(
                "base créée par une version plus récente (schéma {version})"
            ));
        }
        for (i, sql) in MIGRATIONS.iter().enumerate().skip(version) {
            let tx = conn.transaction().map_err(db)?;
            tx.execute_batch(sql).map_err(db)?;
            tx.pragma_update(None, "user_version", i as i64 + 1)
                .map_err(db)?;
            tx.commit().map_err(db)?;
        }
        Ok(Self { conn })
    }

    // ---- projets -------------------------------------------------------------------------

    pub fn create_project(&mut self, name: &str, start_url: &Url) -> Result<i64> {
        self.conn
            .execute(
                "INSERT INTO projects (name, start_url, created_at) VALUES (?1, ?2, ?3)",
                params![name, start_url.as_str(), now()],
            )
            .map_err(db)?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn projects(&self) -> Result<Vec<Project>> {
        self.query_projects("", None)
    }

    pub fn project(&self, id: i64) -> Result<Option<Project>> {
        Ok(self.query_projects("WHERE id = ?1", Some(id.into()))?.pop())
    }

    /// Supprime le projet avec ses audits, pages, liens, problèmes, segments et planification
    /// (atomique). `false` si le projet n'existe pas. L'appelant vérifie qu'aucun audit n'est en cours.
    pub fn delete_project(&mut self, id: i64) -> Result<bool> {
        let tx = self.conn.transaction().map_err(db)?;
        // `crawl_runs` n'a pas de suppression en cascade : on retire les audits d'abord
        // (leurs pages, liens et problèmes partent avec eux).
        tx.execute("DELETE FROM crawl_runs WHERE project_id = ?1", [id])
            .map_err(db)?;
        let n = tx
            .execute("DELETE FROM projects WHERE id = ?1", [id])
            .map_err(db)?;
        tx.commit().map_err(db)?;
        Ok(n > 0)
    }

    /// Configuration d'audit du projet (JSON), `None` tant qu'elle n'a pas été définie.
    pub fn project_config(&self, id: i64) -> Result<Option<String>> {
        self.conn
            .query_row("SELECT config FROM projects WHERE id = ?1", [id], |r| {
                r.get(0)
            })
            .map_err(db)
    }

    /// Remplace la configuration ; `false` si le projet n'existe pas.
    pub fn set_project_config(&mut self, id: i64, config: &str) -> Result<bool> {
        let n = self
            .conn
            .execute(
                "UPDATE projects SET config = ?2 WHERE id = ?1",
                params![id, config],
            )
            .map_err(db)?;
        Ok(n > 0)
    }

    /// Segments du projet, dans l'ordre de création. Une définition illisible est ignorée.
    pub fn segments(&self, project_id: i64) -> Result<Vec<(i64, String, SegmentDef)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name, definition FROM segments WHERE project_id = ?1 ORDER BY id")
            .map_err(db)?;
        let rows = stmt
            .query_map([project_id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get::<_, String>(2)?))
            })
            .map_err(db)?;
        let mut out = Vec::new();
        for row in rows {
            let (id, name, json) = row.map_err(db)?;
            if let Ok(def) = serde_json::from_str(&json) {
                out.push((id, name, def));
            }
        }
        Ok(out)
    }

    pub fn segment(&self, id: i64) -> Result<Option<(i64, String, SegmentDef)>> {
        let row = self
            .conn
            .query_row(
                "SELECT project_id, name, definition FROM segments WHERE id = ?1",
                [id],
                |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                    ))
                },
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                e => Err(db(e)),
            })?;
        row.map(|(pid, name, json)| {
            serde_json::from_str(&json)
                .map(|d| (pid, name, d))
                .map_err(|e| e.to_string())
        })
        .transpose()
    }

    /// Crée un segment ; un nom déjà pris dans le projet est une erreur claire. 20 segments au plus.
    pub fn create_segment(&mut self, project_id: i64, name: &str, def: &SegmentDef) -> Result<i64> {
        const MAX: i64 = 20;
        let n: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM segments WHERE project_id = ?1",
                [project_id],
                |r| r.get(0),
            )
            .map_err(db)?;
        if n >= MAX {
            return Err(format!("{MAX} segments au plus par projet"));
        }
        let json = serde_json::to_string(def).map_err(|e| e.to_string())?;
        self.conn
            .execute(
                "INSERT INTO segments (project_id, name, definition) VALUES (?1, ?2, ?3)",
                params![project_id, name, json],
            )
            .map_err(|e| match e {
                rusqlite::Error::SqliteFailure(f, _)
                    if f.code == rusqlite::ErrorCode::ConstraintViolation =>
                {
                    format!("un segment « {name} » existe déjà")
                }
                e => db(e),
            })?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn delete_segment(&mut self, id: i64) -> Result<bool> {
        let n = self
            .conn
            .execute("DELETE FROM segments WHERE id = ?1", [id])
            .map_err(db)?;
        Ok(n > 0)
    }

    /// Nombre d'URL d'un audit qui correspondent au segment.
    pub fn segment_count(&self, run_id: i64, def: &SegmentDef) -> Result<i64> {
        let q = RowQuery {
            scope: Scope::Report(report("all").expect("rapport « all »")),
            sort: None,
            desc: false,
            columns: vec![column("url").expect("colonne url")],
            search: None,
            segment: Some(def),
            limit: 1,
            offset: 0,
        };
        Ok(self.report_rows(run_id, &q)?.0)
    }

    pub fn schedule(&self, project_id: i64) -> Result<Option<Schedule>> {
        self.conn
            .query_row(
                "SELECT project_id, interval_minutes, enabled, next_run_at, last_run_id FROM schedules WHERE project_id = ?1",
                [project_id],
                read_schedule,
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                e => Err(db(e)),
            })
    }

    /// Crée ou remplace la planification ; la prochaine exécution est dans `interval_minutes`.
    pub fn set_schedule(
        &mut self,
        project_id: i64,
        interval_minutes: i64,
        enabled: bool,
    ) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO schedules (project_id, interval_minutes, enabled, next_run_at)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(project_id) DO UPDATE SET
                    interval_minutes = ?2, enabled = ?3, next_run_at = ?4",
                params![
                    project_id,
                    interval_minutes,
                    enabled,
                    now() + interval_minutes * 60
                ],
            )
            .map(drop)
            .map_err(db)
    }

    pub fn delete_schedule(&mut self, project_id: i64) -> Result<bool> {
        let n = self
            .conn
            .execute("DELETE FROM schedules WHERE project_id = ?1", [project_id])
            .map_err(db)?;
        Ok(n > 0)
    }

    /// Planifications actives dont l'échéance est passée.
    pub fn due_schedules(&self, now: i64) -> Result<Vec<Schedule>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT project_id, interval_minutes, enabled, next_run_at, last_run_id
                 FROM schedules WHERE enabled = 1 AND next_run_at <= ?1 ORDER BY next_run_at",
            )
            .map_err(db)?;
        let rows = stmt.query_map([now], read_schedule).map_err(db)?;
        rows.collect::<std::result::Result<_, _>>().map_err(db)
    }

    /// Un audit planifié vient d'être lancé : on enregistre lequel et on avance l'échéance.
    pub fn schedule_ran(&mut self, project_id: i64, run_id: i64, now: i64) -> Result<()> {
        self.conn
            .execute(
                "UPDATE schedules SET last_run_id = ?2, next_run_at = ?3 + interval_minutes * 60 WHERE project_id = ?1",
                params![project_id, run_id, now],
            )
            .map(drop)
            .map_err(db)
    }

    /// Un audit de ce projet est-il en cours ?
    pub fn has_running_run(&self, project_id: i64) -> Result<bool> {
        self.conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM crawl_runs WHERE project_id = ?1 AND status = 'running')",
                [project_id],
                |r| r.get(0),
            )
            .map_err(db)
    }

    pub fn project_by_name(&self, name: &str) -> Result<Option<Project>> {
        Ok(self
            .query_projects("WHERE name = ?1", Some(name.to_owned().into()))?
            .pop())
    }

    fn query_projects(
        &self,
        filter: &str,
        arg: Option<rusqlite::types::Value>,
    ) -> Result<Vec<Project>> {
        let mut stmt = self
            .conn
            .prepare(&format!(
                "SELECT id, name, start_url, created_at FROM projects {filter} ORDER BY id"
            ))
            .map_err(db)?;
        let map = |r: &Row| {
            Ok(Project {
                id: r.get(0)?,
                name: r.get(1)?,
                start_url: r.get(2)?,
                created_at: r.get(3)?,
            })
        };
        let rows = match arg {
            Some(a) => stmt.query_map([a], map),
            None => stmt.query_map([], map),
        }
        .map_err(db)?;
        rows.collect::<std::result::Result<_, _>>().map_err(db)
    }

    // ---- exécutions ----------------------------------------------------------------------

    /// Ouvre une exécution (état `running`) : ses résultats arriveront avec `finish_run`.
    pub fn create_run(&mut self, project_id: i64, cfg: &CrawlConfig) -> Result<i64> {
        self.conn
            .execute(
                "INSERT INTO crawl_runs (project_id, started_at, settings, status) VALUES (?1, ?2, ?3, 'running')",
                params![project_id, now(), settings_json(cfg)],
            )
            .map_err(db)?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Enregistre les résultats et passe l'exécution à `done`. Refuse si elle n'est plus
    /// `running` (par exemple annulée entre-temps) : rien n'est écrit dans ce cas.
    pub fn finish_run(
        &mut self,
        run_id: i64,
        pages: &[CrawledPage],
        issues: &[Issue],
    ) -> Result<()> {
        let tx = self.conn.transaction().map_err(db)?;
        let changed = tx
            .execute(
                "UPDATE crawl_runs SET status = 'done', finished_at = ?2 WHERE id = ?1 AND status = 'running'",
                params![run_id, now()],
            )
            .map_err(db)?;
        if changed == 0 {
            return Err(format!("l'exécution {run_id} n'est plus en cours"));
        }
        // Les pages enregistrées en direct (`add_live_pages`) sont remplacées par le résultat complet.
        tx.execute("DELETE FROM pages WHERE run_id = ?1", [run_id])
            .map_err(db)?;
        insert_results(&tx, run_id, pages, issues)?;
        tx.commit().map_err(db)
    }

    /// Pages explorées d'une exécution en cours, visibles avant la fin (sans liens ni problèmes :
    /// `finish_run` les remplace). Ignoré si l'exécution n'est plus `running`.
    pub fn add_live_pages(&mut self, run_id: i64, pages: &[CrawledPage]) -> Result<()> {
        let tx = self.conn.transaction().map_err(db)?;
        let running: bool = tx
            .query_row(
                "SELECT status = 'running' FROM crawl_runs WHERE id = ?1",
                [run_id],
                |r| r.get(0),
            )
            .optional()
            .map_err(db)?
            .unwrap_or(false);
        if running {
            let mut stmt = tx.prepare(PAGE_INSERT).map_err(db)?;
            for p in pages {
                insert_page(&mut stmt, run_id, p)?;
            }
        }
        tx.commit().map_err(db)
    }

    /// `failed` ou `cancelled`. Renvoie `false` si l'exécution n'était plus `running`.
    /// Les pages enregistrées en direct sont effacées : un audit inachevé n'a pas de résultats.
    pub fn set_run_status(
        &mut self,
        run_id: i64,
        status: &str,
        error: Option<&str>,
    ) -> Result<bool> {
        let tx = self.conn.transaction().map_err(db)?;
        let changed = tx
            .execute(
                "UPDATE crawl_runs SET status = ?2, finished_at = ?3, error = ?4 WHERE id = ?1 AND status = 'running'",
                params![run_id, status, now(), error],
            )
            .map_err(db)?;
        if changed > 0 {
            tx.execute("DELETE FROM pages WHERE run_id = ?1", [run_id])
                .map_err(db)?;
        }
        tx.commit().map_err(db)?;
        Ok(changed > 0)
    }

    /// Au démarrage du serveur : une exécution encore `running` a été interrompue (ses pages
    /// enregistrées en direct sont effacées).
    pub fn fail_stale_runs(&mut self) -> Result<usize> {
        let tx = self.conn.transaction().map_err(db)?;
        tx.execute(
            "DELETE FROM pages WHERE run_id IN (SELECT id FROM crawl_runs WHERE status = 'running')",
            [],
        )
        .map_err(db)?;
        let n = tx
            .execute(
                "UPDATE crawl_runs SET status = 'failed', finished_at = ?1, error = 'interrompue (arrêt du serveur)' WHERE status = 'running'",
                [now()],
            )
            .map_err(db)?;
        tx.commit().map_err(db)?;
        Ok(n)
    }

    /// Enregistre un audit déjà terminé (atomique) ; crée le projet au besoin.
    pub fn save_run(
        &mut self,
        project: &str,
        start: &Url,
        cfg: &CrawlConfig,
        pages: &[CrawledPage],
        issues: &[Issue],
    ) -> Result<i64> {
        let tx = self.conn.transaction().map_err(db)?;
        tx.execute(
            "INSERT OR IGNORE INTO projects (name, start_url, created_at) VALUES (?1, ?2, ?3)",
            params![project, start.as_str(), now()],
        )
        .map_err(db)?;
        let project_id: i64 = tx
            .query_row("SELECT id FROM projects WHERE name = ?1", [project], |r| {
                r.get(0)
            })
            .map_err(db)?;
        tx.execute(
            "INSERT INTO crawl_runs (project_id, started_at, finished_at, settings, status) VALUES (?1, ?2, ?2, ?3, 'done')",
            params![project_id, now(), settings_json(cfg)],
        )
        .map_err(db)?;
        let run_id = tx.last_insert_rowid();
        insert_results(&tx, run_id, pages, issues)?;
        tx.commit().map_err(db)?;
        Ok(run_id)
    }

    /// Exécutions, de la plus récente à la plus ancienne.
    pub fn runs(&self) -> Result<Vec<RunInfo>> {
        self.query_runs(None, None)
    }

    /// Dernier audit de chaque projet (les projets sans audit sont absents).
    pub fn latest_runs(&self) -> Result<Vec<RunInfo>> {
        let mut stmt = self
            .conn
            .prepare("SELECT MAX(id) FROM crawl_runs GROUP BY project_id")
            .map_err(db)?;
        let ids: Vec<i64> = stmt
            .query_map([], |r| r.get(0))
            .map_err(db)?
            .collect::<std::result::Result<_, _>>()
            .map_err(db)?;
        ids.into_iter()
            .filter_map(|id| self.run(id).transpose())
            .collect()
    }

    pub fn runs_of_project(&self, project_id: i64) -> Result<Vec<RunInfo>> {
        self.query_runs(Some(project_id), None)
    }

    pub fn run(&self, run_id: i64) -> Result<Option<RunInfo>> {
        Ok(self.query_runs(None, Some(run_id))?.pop())
    }

    fn query_runs(&self, project: Option<i64>, run: Option<i64>) -> Result<Vec<RunInfo>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT r.id, r.project_id, p.name, p.start_url, r.status, r.started_at, r.finished_at, r.error,
                    (SELECT COUNT(*) FROM pages WHERE run_id = r.id),
                    (SELECT COUNT(*) FROM issues WHERE run_id = r.id AND severity = 'Error'),
                    (SELECT COUNT(*) FROM issues WHERE run_id = r.id AND severity = 'Warning')
                 FROM crawl_runs r JOIN projects p ON p.id = r.project_id
                 WHERE (?1 IS NULL OR r.project_id = ?1) AND (?2 IS NULL OR r.id = ?2)
                 ORDER BY r.id DESC",
            )
            .map_err(db)?;
        let rows = stmt
            .query_map(params![project, run], |r| {
                Ok(RunInfo {
                    id: r.get(0)?,
                    project_id: r.get(1)?,
                    project: r.get(2)?,
                    start_url: r.get(3)?,
                    status: r.get(4)?,
                    started_at: r.get(5)?,
                    finished_at: r.get(6)?,
                    error: r.get(7)?,
                    pages: r.get(8)?,
                    errors: r.get(9)?,
                    warnings: r.get(10)?,
                })
            })
            .map_err(db)?;
        rows.collect::<std::result::Result<_, _>>().map_err(db)
    }

    // ---- lecture -------------------------------------------------------------------------

    /// Pages d'une exécution, avec leurs liens sortants.
    pub fn load_run(&self, run_id: i64) -> Result<(Vec<CrawledPage>, Vec<Issue>)> {
        if self.run(run_id)?.is_none() {
            return Err(format!("exécution {run_id} introuvable"));
        }

        // Une seule allocation par URL distincte, comme dans le crawler.
        let mut interner = UrlInterner::default();
        let mut links: HashMap<i64, Vec<Arc<Url>>> = HashMap::new();
        let mut stmt = self
            .conn
            .prepare(
                "SELECT page_id, target FROM links
                 WHERE page_id IN (SELECT id FROM pages WHERE run_id = ?1)",
            )
            .map_err(db)?;
        for row in stmt
            .query_map([run_id], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
            })
            .map_err(db)?
        {
            let (page_id, target) = row.map_err(db)?;
            links
                .entry(page_id)
                .or_default()
                .push(interner.intern(parse_url(&target)?));
        }

        let mut stmt = self
            .conn
            .prepare(&format!(
                "SELECT {PAGE_COLS} FROM pages WHERE run_id = ?1 ORDER BY id"
            ))
            .map_err(db)?;
        let mut pages = Vec::new();
        for row in stmt.query_map([run_id], read_page_row).map_err(db)? {
            let raw = row.map_err(db)?;
            let page_links = links.remove(&raw.0).unwrap_or_default();
            pages.push(assemble_page(raw, page_links)?);
        }

        let mut stmt = self
            .conn
            .prepare(&format!(
                "SELECT {ISSUE_COLS} FROM issues WHERE run_id = ?1 ORDER BY id"
            ))
            .map_err(db)?;
        let mut issues = Vec::new();
        for row in stmt.query_map([run_id], read_issue_row).map_err(db)? {
            issues.push(assemble_issue(row.map_err(db)?)?);
        }
        Ok((pages, issues))
    }

    /// Une page de résultats (total avant pagination, pages sans leurs liens).
    pub fn pages_page(&self, run_id: i64, q: &PageQuery) -> Result<(i64, Vec<CrawledPage>)> {
        const FILTER: &str = "run_id = ?1
            AND (?2 IS NULL OR instr(lower(url), lower(?2)) > 0
                 OR instr(lower(coalesce(title, '')), lower(?2)) > 0)
            AND (?3 IS NULL OR status = ?3)";
        let total = self
            .conn
            .query_row(
                &format!("SELECT COUNT(*) FROM pages WHERE {FILTER}"),
                params![run_id, q.search, q.status],
                |r| r.get(0),
            )
            .map_err(db)?;
        let mut stmt = self
            .conn
            .prepare(&format!(
                "SELECT {PAGE_COLS} FROM pages WHERE {FILTER} ORDER BY id LIMIT ?4 OFFSET ?5"
            ))
            .map_err(db)?;
        let mut pages = Vec::new();
        let rows = stmt
            .query_map(
                params![run_id, q.search, q.status, q.limit, q.offset],
                read_page_row,
            )
            .map_err(db)?;
        for row in rows {
            pages.push(assemble_page(row.map_err(db)?, Vec::new())?);
        }
        Ok((total, pages))
    }

    /// Problèmes filtrés et paginés : erreurs d'abord, puis avertissements, puis infos.
    pub fn issues_page(&self, run_id: i64, q: &IssueQuery) -> Result<(i64, Vec<Issue>)> {
        const FILTER: &str =
            "run_id = ?1 AND (?2 IS NULL OR severity = ?2) AND (?3 IS NULL OR rule = ?3)";
        let total = self
            .conn
            .query_row(
                &format!("SELECT COUNT(*) FROM issues WHERE {FILTER}"),
                params![run_id, q.severity, q.rule],
                |r| r.get(0),
            )
            .map_err(db)?;
        let mut stmt = self
            .conn
            .prepare(&format!(
                "SELECT {ISSUE_COLS} FROM issues WHERE {FILTER}
                 ORDER BY CASE severity WHEN 'Error' THEN 0 WHEN 'Warning' THEN 1 ELSE 2 END, rule, url, id
                 LIMIT ?4 OFFSET ?5"
            ))
            .map_err(db)?;
        let mut issues = Vec::new();
        let rows = stmt
            .query_map(
                params![run_id, q.severity, q.rule, q.limit, q.offset],
                read_issue_row,
            )
            .map_err(db)?;
        for row in rows {
            issues.push(assemble_issue(row.map_err(db)?)?);
        }
        Ok((total, issues))
    }

    /// Problèmes regroupés par règle : (règle, gravité, nombre).
    pub fn issue_summary(&self, run_id: i64) -> Result<Vec<(String, String, i64)>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT rule, severity, COUNT(*) FROM issues WHERE run_id = ?1
                 GROUP BY rule, severity
                 ORDER BY CASE severity WHEN 'Error' THEN 0 WHEN 'Warning' THEN 1 ELSE 2 END, COUNT(*) DESC, rule",
            )
            .map_err(db)?;
        let rows = stmt
            .query_map([run_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .map_err(db)?;
        rows.collect::<std::result::Result<_, _>>().map_err(db)
    }

    /// Une page par son URL exacte, sans ses liens (voir `outlinks`/`inlinks`).
    pub fn page_by_url(&self, run_id: i64, url: &str) -> Result<Option<CrawledPage>> {
        let mut stmt = self
            .conn
            .prepare(&format!(
                "SELECT {PAGE_COLS} FROM pages WHERE run_id = ?1 AND url = ?2 LIMIT 1"
            ))
            .map_err(db)?;
        let raw = stmt
            .query_map(params![run_id, url], read_page_row)
            .map_err(db)?
            .next()
            .transpose()
            .map_err(db)?;
        raw.map(|r| assemble_page(r, Vec::new())).transpose()
    }

    /// Liens sortants d'une page : (cible, cible crawlée ?, statut de la cible).
    pub fn outlinks(&self, run_id: i64, url: &str) -> Result<Vec<(String, bool, Option<u16>)>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT l.target, t.id IS NOT NULL, t.status FROM links l
                 JOIN pages s ON s.id = l.page_id
                 LEFT JOIN pages t ON t.run_id = s.run_id AND t.url = l.target
                 WHERE s.run_id = ?1 AND s.url = ?2 ORDER BY l.rowid",
            )
            .map_err(db)?;
        let rows = stmt
            .query_map(params![run_id, url], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .map_err(db)?;
        rows.collect::<std::result::Result<_, _>>().map_err(db)
    }

    /// Pages qui lient vers `url` (hors elle-même) : (total, `limit` premières sources avec
    /// leur statut HTTP et leur title).
    pub fn inlinks(&self, run_id: i64, url: &str, limit: i64) -> Result<(i64, Vec<Inlink>)> {
        const FROM: &str = "FROM links l JOIN pages s ON s.id = l.page_id
             WHERE s.run_id = ?1 AND l.target = ?2 AND s.url != ?2";
        let total = self
            .conn
            .query_row(
                &format!("SELECT COUNT(DISTINCT s.id) {FROM}"),
                params![run_id, url],
                |r| r.get(0),
            )
            .map_err(db)?;
        let mut stmt = self
            .conn
            .prepare(&format!(
                "SELECT s.url, s.status, s.title {FROM} GROUP BY s.id ORDER BY s.id LIMIT ?3"
            ))
            .map_err(db)?;
        let rows = stmt
            .query_map(params![run_id, url, limit], |r| {
                Ok(Inlink {
                    url: r.get(0)?,
                    status: r.get(1)?,
                    title: r.get(2)?,
                })
            })
            .map_err(db)?;
        Ok((
            total,
            rows.collect::<std::result::Result<_, _>>().map_err(db)?,
        ))
    }

    pub fn issues_of_url(&self, run_id: i64, url: &str) -> Result<Vec<Issue>> {
        let mut stmt = self
            .conn
            .prepare(&format!(
                "SELECT {ISSUE_COLS} FROM issues WHERE run_id = ?1 AND url = ?2 ORDER BY id"
            ))
            .map_err(db)?;
        let mut issues = Vec::new();
        for row in stmt
            .query_map(params![run_id, url], read_issue_row)
            .map_err(db)?
        {
            issues.push(assemble_issue(row.map_err(db)?)?);
        }
        Ok(issues)
    }

    /// Vue d'ensemble du maillage interne ; les listes sont limitées à `limit` éléments.
    pub fn link_stats(&self, run_id: i64, limit: i64) -> Result<LinkStats> {
        // Tout vient des compteurs précalculés par page (`inlinks`, `outlinks`) : aucune jointure.
        let one = |sql: &str| -> Result<i64> {
            self.conn.query_row(sql, [run_id], |r| r.get(0)).map_err(db)
        };
        let pairs = |sql: &str| -> Result<Vec<(String, i64)>> {
            let mut stmt = self.conn.prepare(sql).map_err(db)?;
            let rows = stmt
                .query_map(params![run_id, limit], |r| Ok((r.get(0)?, r.get(1)?)))
                .map_err(db)?;
            rows.collect::<std::result::Result<_, _>>().map_err(db)
        };
        const ORPHAN: &str =
            "FROM pages WHERE run_id = ?1 AND depth > 0 AND coalesce(inlinks, 0) = 0";
        let mut stmt = self
            .conn
            .prepare(
                "SELECT depth, COUNT(*) FROM pages WHERE run_id = ?1 GROUP BY depth ORDER BY depth",
            )
            .map_err(db)?;
        let depths = stmt
            .query_map([run_id], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(db)?
            .collect::<std::result::Result<_, _>>()
            .map_err(db)?;
        let mut stmt = self
            .conn
            .prepare(&format!("SELECT url {ORPHAN} ORDER BY id LIMIT ?2"))
            .map_err(db)?;
        let orphans = stmt
            .query_map(params![run_id, limit], |r| r.get(0))
            .map_err(db)?
            .collect::<std::result::Result<_, _>>()
            .map_err(db)?;

        Ok(LinkStats {
            depths,
            internal_links: one("SELECT COALESCE(SUM(outlinks), 0) FROM pages WHERE run_id = ?1")?,
            top_linked: pairs(
                "SELECT url, inlinks FROM pages WHERE run_id = ?1 AND inlinks > 0
                 ORDER BY inlinks DESC, url LIMIT ?2",
            )?,
            top_linking: pairs(
                "SELECT url, outlinks FROM pages WHERE run_id = ?1 AND outlinks > 0
                 ORDER BY outlinks DESC, url LIMIT ?2",
            )?,
            orphans_total: one(&format!("SELECT COUNT(*) {ORPHAN}"))?,
            orphans,
        })
    }

    /// Nœuds du graphe : (URL, profondeur, statut), au plus `limit`, et le nombre total de pages.
    /// Requête légère : elle ne charge aucune des colonnes volumineuses (en-têtes, images, JSON-LD).
    pub fn graph_nodes(&self, run_id: i64, limit: i64) -> Result<(i64, Vec<GraphNode>)> {
        let total = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM pages WHERE run_id = ?1",
                [run_id],
                |r| r.get(0),
            )
            .map_err(db)?;
        let mut stmt = self
            .conn
            .prepare("SELECT url, depth, status FROM pages WHERE run_id = ?1 ORDER BY id LIMIT ?2")
            .map_err(db)?;
        let rows = stmt
            .query_map(params![run_id, limit], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .map_err(db)?;
        Ok((
            total,
            rows.collect::<std::result::Result<_, _>>().map_err(db)?,
        ))
    }

    /// Liens internes entre pages crawlées : (URL source, URL cible), au plus `limit`.
    pub fn edges(&self, run_id: i64, limit: i64) -> Result<Vec<(String, String)>> {
        let mut stmt = self
            .conn
            // Parcours imposé (pages, puis leurs liens, puis « la cible est crawlée ? ») : le `LIMIT`
            // arrête tout dès que le plafond est atteint. Un ordre explicite (`ORDER BY l.rowid`)
            // faisait trier tous les liens de l'audit d'abord (0,8 s pour 50 000 sur 900 000 liens ;
            // 0,02 s ainsi). L'ordre est celui de l'index (URL) : stable, sans autre garantie.
            .prepare(
                "SELECT s.url, l.target FROM pages s CROSS JOIN links l ON l.page_id = s.id
                 WHERE s.run_id = ?1
                   AND EXISTS (SELECT 1 FROM pages t WHERE t.run_id = ?1 AND t.url = l.target)
                 LIMIT ?2",
            )
            .map_err(db)?;
        let rows = stmt
            .query_map(params![run_id, limit], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(db)?;
        rows.collect::<std::result::Result<_, _>>().map_err(db)
    }
}

const PAGE_COLS: &str = "id, url, depth, status, redirect_to, is_html, in_sitemap, title, description, h1, canonical, robots, error, \
    ttfb_ms, total_ms, size, content_type, headers, content_hash, word_count, lang, viewport, h2, hreflang, images, images_total, json_ld, nofollow_links, extracted, simhash, amp_html, is_amp, microdata, microdata_untyped, sitemap_lastmod, anchor_empty, anchor_generic, external_broken, ai_bots_blocked";

/// Liste vide → NULL (les pages sans liste n'occupent rien dans la base).
fn list_json<T: serde::Serialize>(v: &[T]) -> Result<Option<String>> {
    if v.is_empty() {
        return Ok(None);
    }
    serde_json::to_string(v)
        .map(Some)
        .map_err(|e| e.to_string())
}

/// Extractions en objet JSON `{nom: [valeurs]}` (NULL si aucune).
fn extracted_json(v: &[(String, Vec<String>)]) -> Result<Option<String>> {
    if v.is_empty() {
        return Ok(None);
    }
    let map: std::collections::BTreeMap<_, _> = v.iter().cloned().collect();
    serde_json::to_string(&map)
        .map(Some)
        .map_err(|e| e.to_string())
}

/// Lecture tolérante : NULL (audit d'avant la v3) ou JSON illisible → liste vide.
fn json_col<T: serde::de::DeserializeOwned + Default>(r: &Row, i: usize) -> rusqlite::Result<T> {
    Ok(r.get::<_, Option<String>>(i)?
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default())
}

fn read_facts(r: &Row) -> rusqlite::Result<PageFacts> {
    Ok(PageFacts {
        ttfb_ms: r.get(13)?,
        total_ms: r.get(14)?,
        size: r.get::<_, Option<i64>>(15)?.map(|n| n as u64),
        content_type: r.get(16)?,
        headers: json_col(r, 17)?,
        content_hash: r.get(18)?,
        word_count: r.get(19)?,
        lang: r.get(20)?,
        viewport: r.get(21)?,
        h2: json_col(r, 22)?,
        hreflang: json_col(r, 23)?,
        images: json_col(r, 24)?,
        images_total: r.get::<_, Option<u32>>(25)?.unwrap_or_default(),
        json_ld: json_col(r, 26)?,
        nofollow_links: r.get::<_, Option<u32>>(27)?.unwrap_or_default(),
        extracted: json_col::<std::collections::BTreeMap<String, Vec<String>>>(r, 28)?
            .into_iter()
            .collect(),
        simhash: r.get(29)?,
        amp_html: r.get(30)?,
        is_amp: r.get::<_, Option<bool>>(31)?.unwrap_or_default(),
        microdata: r.get::<_, Option<u32>>(32)?.unwrap_or_default(),
        microdata_untyped: r.get::<_, Option<u32>>(33)?.unwrap_or_default(),
        sitemap_lastmod: r.get(34)?,
        anchor_empty: r.get::<_, Option<u32>>(35)?.unwrap_or_default(),
        anchor_generic: r.get::<_, Option<u32>>(36)?.unwrap_or_default(),
        external_broken: json_col(r, 37)?,
        ai_bots_blocked: json_col(r, 38)?,
    })
}
const ISSUE_COLS: &str = "rule, severity, url, message";

struct PageMetaRow {
    title: Option<String>,
    description: Option<String>,
    h1: String,
    canonical: Option<String>,
    robots: Option<String>,
}

type RawPage = (
    i64,
    String,
    i64,
    Option<u16>,
    Option<String>,
    bool,
    bool,
    PageMetaRow,
    Option<String>,
    PageFacts,
);
type RawIssue = (String, String, String, String);

fn read_page_row(r: &Row) -> rusqlite::Result<RawPage> {
    Ok((
        r.get(0)?,
        r.get(1)?,
        r.get(2)?,
        r.get(3)?,
        r.get(4)?,
        r.get(5)?,
        r.get(6)?,
        PageMetaRow {
            title: r.get(7)?,
            description: r.get(8)?,
            h1: r.get(9)?,
            canonical: r.get(10)?,
            robots: r.get(11)?,
        },
        r.get(12)?,
        read_facts(r)?,
    ))
}

fn read_issue_row(r: &Row) -> rusqlite::Result<RawIssue> {
    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
}

fn assemble_page(raw: RawPage, links: Vec<Arc<Url>>) -> Result<CrawledPage> {
    let (_, url, depth, status, redirect_to, is_html, in_sitemap, m, error, facts) = raw;
    Ok(CrawledPage {
        facts,
        in_sitemap,
        is_html,
        meta: PageMeta {
            title: m.title,
            description: m.description,
            h1: serde_json::from_str(&m.h1).map_err(|e| e.to_string())?,
            canonical: m.canonical.as_deref().map(parse_url).transpose()?,
            robots: m.robots,
        },
        url: parse_url(&url)?,
        depth: depth as usize,
        status,
        redirect_to: redirect_to.as_deref().map(parse_url).transpose()?,
        links,
        error,
    })
}

fn assemble_issue((rule, severity, url, message): RawIssue) -> Result<Issue> {
    Ok(Issue {
        rule: intern(rule),
        severity: match severity.as_str() {
            "Error" => Severity::Error,
            "Warning" => Severity::Warning,
            _ => Severity::Info,
        },
        url: parse_url(&url)?,
        message,
    })
}

/// Page source d'un lien entrant.
#[derive(Debug, PartialEq)]
pub struct Inlink {
    pub url: String,
    pub status: Option<u16>,
    pub title: Option<String>,
}

const PAGE_INSERT: &str = "INSERT INTO pages (run_id, url, depth, status, redirect_to, is_html, in_sitemap, \
     title, description, h1, canonical, robots, error, ttfb_ms, total_ms, size, \
     content_type, headers, content_hash, word_count, lang, viewport, h2, hreflang, \
     images, images_total, json_ld, nofollow_links, extracted, simhash, amp_html, is_amp, microdata, microdata_untyped, sitemap_lastmod, anchor_empty, anchor_generic, external_broken, ai_bots_blocked) \
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, \
     ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30, ?31, ?32, ?33, ?34, ?35, ?36, ?37, ?38, ?39)";

/// Une ligne de `pages` (sans ses liens) ; renvoie son identifiant.
fn insert_page(stmt: &mut rusqlite::Statement, run_id: i64, p: &CrawledPage) -> Result<i64> {
    let h1 = serde_json::to_string(&p.meta.h1).map_err(|e| e.to_string())?;
    let f = &p.facts;
    stmt.insert(params![
        run_id,
        p.url.as_str(),
        p.depth as i64,
        p.status,
        p.redirect_to.as_ref().map(Url::as_str),
        p.is_html,
        p.in_sitemap,
        p.meta.title,
        p.meta.description,
        h1,
        p.meta.canonical.as_ref().map(Url::as_str),
        p.meta.robots,
        p.error,
        f.ttfb_ms,
        f.total_ms,
        f.size.map(|n| n as i64),
        f.content_type,
        list_json(&f.headers)?,
        f.content_hash,
        f.word_count,
        f.lang,
        f.viewport,
        list_json(&f.h2)?,
        list_json(&f.hreflang)?,
        list_json(&f.images)?,
        f.images_total,
        list_json(&f.json_ld)?,
        f.nofollow_links,
        extracted_json(&f.extracted)?,
        f.simhash,
        f.amp_html,
        f.is_amp,
        f.microdata,
        f.microdata_untyped,
        f.sitemap_lastmod,
        f.anchor_empty,
        f.anchor_generic,
        list_json(&f.external_broken)?,
        list_json(&f.ai_bots_blocked)?,
    ])
    .map_err(db)
}

fn insert_results(
    tx: &Transaction,
    run_id: i64,
    pages: &[CrawledPage],
    issues: &[Issue],
) -> Result<()> {
    let mut page_stmt = tx.prepare(PAGE_INSERT).map_err(db)?;
    let mut link_stmt = tx
        .prepare("INSERT INTO links (page_id, target) VALUES (?1, ?2)")
        .map_err(db)?;
    let mut page_ids = Vec::with_capacity(pages.len());
    for p in pages {
        let page_id = insert_page(&mut page_stmt, run_id, p)?;
        page_ids.push(page_id);
        for l in &p.links {
            link_stmt
                .execute(params![page_id, l.as_str()])
                .map_err(db)?;
        }
    }
    // Liens internes précalculés (voir la migration v8). Calculés en mémoire sur les pages déjà
    // en main : l'ancien UPDATE à sous-requêtes parcourait `links` de tout l'historique.
    let by_url: HashMap<&str, usize> = pages
        .iter()
        .enumerate()
        .map(|(i, p)| (p.url.as_str(), i))
        .collect();
    let mut inlinks = vec![0i64; pages.len()];
    let mut outlinks = vec![0i64; pages.len()];
    let mut seen = HashSet::new();
    for (i, p) in pages.iter().enumerate() {
        seen.clear();
        for l in &p.links {
            match by_url.get(l.as_str()) {
                Some(&j) if j != i && seen.insert(j) => {
                    inlinks[j] += 1;
                    outlinks[i] += 1;
                }
                _ => {}
            }
        }
    }
    let mut count_stmt = tx
        .prepare("UPDATE pages SET inlinks = ?2, outlinks = ?3 WHERE id = ?1")
        .map_err(db)?;
    for (k, id) in page_ids.iter().enumerate() {
        count_stmt
            .execute(params![id, inlinks[k], outlinks[k]])
            .map_err(db)?;
    }
    let mut issue_stmt = tx
        .prepare(
            "INSERT INTO issues (run_id, rule, severity, url, message) VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .map_err(db)?;
    for i in issues {
        issue_stmt
            .execute(params![
                run_id,
                i.rule,
                i.severity.as_str(),
                i.url.as_str(),
                i.message
            ])
            .map_err(db)?;
    }
    Ok(())
}

fn settings_json(cfg: &CrawlConfig) -> String {
    json!({
        "max_pages": cfg.max_pages, "max_depth": cfg.max_depth, "concurrency": cfg.concurrency,
        "timeout_secs": cfg.timeout.as_secs(), "delay_ms": cfg.delay.as_millis() as u64, "max_body_bytes": cfg.max_body_bytes,
        "user_agent": cfg.user_agent, "respect_robots": cfg.respect_robots,
        "allow_private": cfg.allow_private,
    })
    .to_string()
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

fn parse_url(s: &str) -> Result<Url> {
    Url::parse(s).map_err(|e| format!("URL invalide en base ({s}): {e}"))
}

/// `Issue::rule` est un `&'static str` : on internalise les noms relus de la base.
// ponytail: fuite bornée par le nombre de règles distinctes ; passer `rule` en `String` si ça gêne.
fn intern(s: String) -> &'static str {
    static POOL: OnceLock<Mutex<HashSet<&'static str>>> = OnceLock::new();
    let mut pool = POOL.get_or_init(Default::default).lock().expect("pool");
    if let Some(known) = pool.get(s.as_str()) {
        return known;
    }
    let leaked: &'static str = Box::leak(s.into_boxed_str());
    pool.insert(leaked);
    leaked
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u(p: &str) -> Url {
        Url::parse(&format!("http://a.test{p}")).unwrap()
    }

    fn sample() -> (Vec<CrawledPage>, Vec<Issue>) {
        let page = CrawledPage {
            facts: Default::default(),
            in_sitemap: true,
            is_html: true,
            meta: PageMeta {
                title: Some("T".into()),
                description: None,
                h1: vec!["a".into(), "b".into()],
                canonical: Some(u("/c")),
                robots: Some("noindex".into()),
            },
            url: u("/"),
            depth: 0,
            status: Some(200),
            redirect_to: None,
            links: vec![Arc::new(u("/x")), Arc::new(u("/y?q=1"))],
            error: None,
        };
        let broken = CrawledPage {
            facts: Default::default(),
            in_sitemap: false,
            is_html: false,
            meta: PageMeta::default(),
            url: u("/x"),
            depth: 1,
            status: None,
            redirect_to: Some(u("/z")),
            links: vec![],
            error: Some("boom".into()),
        };
        let issue = Issue {
            rule: "http-error",
            severity: Severity::Error,
            url: u("/x"),
            message: "statut".into(),
        };
        (vec![page, broken], vec![issue])
    }

    #[test]
    fn roundtrip_and_runs_are_never_overwritten() {
        let mut store = Store::open_in_memory().unwrap();
        let (pages, issues) = sample();
        let cfg = CrawlConfig::default();
        let first = store
            .save_run("site", &u("/"), &cfg, &pages, &issues)
            .unwrap();
        let second = store
            .save_run("site", &u("/"), &cfg, &pages[..1], &[])
            .unwrap();
        assert_ne!(first, second);

        let (got_pages, got_issues) = store.load_run(first).unwrap();
        assert_eq!(got_pages.len(), 2);
        let p = &got_pages[0];
        assert_eq!(
            (p.url.as_str(), p.depth, p.status),
            ("http://a.test/", 0, Some(200))
        );
        assert_eq!(p.meta.h1, ["a", "b"]);
        assert_eq!(p.meta.canonical, Some(u("/c")));
        assert_eq!(
            p.links.iter().map(|l| l.as_str()).collect::<Vec<_>>(),
            ["http://a.test/x", "http://a.test/y?q=1"]
        );
        assert!(p.in_sitemap && p.is_html && p.meta.description.is_none());
        assert_eq!(got_pages[1].error.as_deref(), Some("boom"));
        assert_eq!(got_pages[1].redirect_to, Some(u("/z")));
        assert_eq!(got_issues.len(), 1);
        assert_eq!(
            (got_issues[0].rule, got_issues[0].severity),
            ("http-error", Severity::Error)
        );

        let runs = store.runs().unwrap();
        assert_eq!(
            runs.iter().map(|r| r.id).collect::<Vec<_>>(),
            [second, first]
        );
        assert_eq!((runs[1].pages, runs[1].errors, runs[1].warnings), (2, 1, 0));
        assert_eq!((runs[1].status.as_str(), runs[0].pages), ("done", 1));
        assert!(store.load_run(999).is_err());
    }

    #[test]
    fn reopening_a_file_keeps_data_and_migrates_once() {
        let path = std::env::temp_dir().join(format!("callistseo-test-{}.db", std::process::id()));
        let (pages, issues) = sample();
        let id = Store::open(&path)
            .unwrap()
            .save_run("site", &u("/"), &CrawlConfig::default(), &pages, &issues)
            .unwrap();
        let reopened = Store::open(&path).unwrap();
        assert_eq!(reopened.load_run(id).unwrap().0.len(), 2);
        for ext in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{ext}", path.display()));
        }
    }

    #[test]
    fn run_lifecycle_and_projects() {
        let mut store = Store::open_in_memory().unwrap();
        let cfg = CrawlConfig::default();
        let pid = store.create_project("p", &u("/")).unwrap();
        assert!(store.create_project("p", &u("/")).is_err()); // nom unique
        assert_eq!(store.project_by_name("p").unwrap().unwrap().id, pid);
        assert!(store.project(999).unwrap().is_none());

        let (pages, issues) = sample();
        let done = store.create_run(pid, &cfg).unwrap();
        assert_eq!(store.run(done).unwrap().unwrap().status, "running");
        // Pages en direct : visibles pendant l'audit, remplacées (pas doublées) à la fin.
        store.add_live_pages(done, &pages[..2]).unwrap();
        assert_eq!(store.run(done).unwrap().unwrap().pages, 2);
        store.finish_run(done, &pages, &issues).unwrap();
        assert_eq!(store.run(done).unwrap().unwrap().pages, pages.len() as i64);
        store.add_live_pages(done, &pages).unwrap(); // lot tardif : ignoré
        assert_eq!(store.load_run(done).unwrap().0.len(), pages.len());
        assert_eq!(store.run(done).unwrap().unwrap().status, "done");
        assert!(store.finish_run(done, &pages, &issues).is_err()); // déjà terminée

        let cancelled = store.create_run(pid, &cfg).unwrap();
        store.add_live_pages(cancelled, &pages).unwrap();
        assert!(store.set_run_status(cancelled, "cancelled", None).unwrap());
        assert!(!store.set_run_status(cancelled, "failed", None).unwrap()); // état final
        assert!(store.finish_run(cancelled, &pages, &issues).is_err());
        assert_eq!(store.load_run(cancelled).unwrap().0.len(), 0);

        let stale = store.create_run(pid, &cfg).unwrap();
        store.add_live_pages(stale, &pages).unwrap();
        assert_eq!(store.fail_stale_runs().unwrap(), 1);
        let r = store.run(stale).unwrap().unwrap();
        assert_eq!(r.pages, 0);
        assert_eq!(r.status, "failed");
        assert!(r.error.unwrap().contains("interrompue"));
        assert_eq!(store.runs_of_project(pid).unwrap().len(), 3);
        assert!(store.runs_of_project(999).unwrap().is_empty());
    }

    #[test]
    fn page_detail_and_link_stats() {
        let mut store = Store::open_in_memory().unwrap();
        let (mut pages, issues) = sample();
        // / -> /x, /y?q=1 ; /x -> / ; /lonely n'a aucun lien entrant
        pages[1].links = vec![Arc::new(u("/"))];
        let mut lonely = pages[1].clone();
        (lonely.url, lonely.links, lonely.depth) = (u("/lonely"), vec![], 2);
        pages.push(lonely);
        let run = store
            .save_run("s", &u("/"), &CrawlConfig::default(), &pages, &issues)
            .unwrap();

        assert!(store
            .page_by_url(run, "http://a.test/nope")
            .unwrap()
            .is_none());
        let x = store.page_by_url(run, "http://a.test/x").unwrap().unwrap();
        assert_eq!((x.depth, x.links.len()), (1, 0)); // les liens se lisent à part

        assert_eq!(
            store.outlinks(run, "http://a.test/").unwrap(),
            [
                ("http://a.test/x".to_string(), true, None),
                ("http://a.test/y?q=1".to_string(), false, None)
            ]
        );
        let (total, from) = store.inlinks(run, "http://a.test/x", 10).unwrap();
        assert_eq!(total, 1);
        assert_eq!(
            from,
            [Inlink {
                url: "http://a.test/".into(),
                status: Some(200),
                title: Some("T".into())
            }]
        );
        assert_eq!(store.inlinks(run, "http://a.test/lonely", 10).unwrap().0, 0);
        assert_eq!(
            store.issues_of_url(run, "http://a.test/x").unwrap().len(),
            1
        );
        assert!(store
            .issues_of_url(run, "http://a.test/")
            .unwrap()
            .is_empty());

        let s = store.link_stats(run, 10).unwrap();
        assert_eq!(s.depths, [(0, 1), (1, 1), (2, 1)]);
        assert_eq!(s.internal_links, 2); // / -> /x et /x -> /
        assert_eq!(s.orphans, ["http://a.test/lonely"]);
        assert_eq!(s.orphans_total, 1);
        assert_eq!(s.top_linked.len(), 2);
        assert_eq!(s.top_linked[0].1, 1);
        assert_eq!(store.link_stats(run, 0).unwrap().orphans_total, 1); // total indépendant de la limite
    }

    #[test]
    fn pagination_filters_summary_and_edges() {
        let mut store = Store::open_in_memory().unwrap();
        let (pages, mut issues) = sample();
        issues.push(Issue {
            rule: "title-missing",
            severity: Severity::Warning,
            url: u("/"),
            message: "m".into(),
        });
        let run = store
            .save_run("s", &u("/"), &CrawlConfig::default(), &pages, &issues)
            .unwrap();

        let (total, got) = store
            .pages_page(
                run,
                &PageQuery {
                    limit: 1,
                    offset: 1,
                    ..PageQuery::default()
                },
            )
            .unwrap();
        assert_eq!((total, got.len(), got[0].url.path()), (2, 1, "/x"));
        let q = |search: &str, status| PageQuery {
            search: Some(search.into()),
            status,
            limit: 10,
            offset: 0,
        };
        assert_eq!(store.pages_page(run, &q("T", None)).unwrap().0, 2); // « t » dans l'URL et le title
        assert_eq!(store.pages_page(run, &q("%", None)).unwrap().0, 0); // pas de joker LIKE
        assert_eq!(store.pages_page(run, &q("", Some(200))).unwrap().0, 1);
        assert_eq!(store.pages_page(run, &q("", Some(404))).unwrap().0, 0);

        let (total, got) = store
            .issues_page(
                run,
                &IssueQuery {
                    limit: 10,
                    ..IssueQuery::default()
                },
            )
            .unwrap();
        assert_eq!((total, got[0].severity), (2, Severity::Error)); // erreurs d'abord
        let only = IssueQuery {
            rule: Some("title-missing".into()),
            limit: 10,
            ..IssueQuery::default()
        };
        assert_eq!(store.issues_page(run, &only).unwrap().0, 1);
        let summary = store.issue_summary(run).unwrap();
        assert_eq!(
            summary,
            [
                ("http-error".to_string(), "Error".to_string(), 1),
                ("title-missing".to_string(), "Warning".to_string(), 1)
            ]
        );

        // `/` pointe vers `/x` (crawlée) et `/y?q=1` (non crawlée) : une seule arête.
        assert_eq!(
            store.edges(run, 100).unwrap(),
            [("http://a.test/".to_string(), "http://a.test/x".to_string())]
        );
    }

    #[test]
    fn page_facts_survive_a_save_and_reload() {
        let (mut pages, _) = sample();
        pages[0].facts = PageFacts {
            ttfb_ms: Some(120),
            total_ms: Some(340),
            size: Some(2048),
            content_type: Some("text/html; charset=utf-8".into()),
            headers: vec![("x-robots-tag".into(), "noindex".into())],
            content_hash: Some("00ff00ff00ff00ff".into()),
            simhash: Some("0f0f0f0f0f0f0f0f".into()),
            amp_html: Some("https://e.com/amp".into()),
            is_amp: true,
            microdata: 2,
            microdata_untyped: 1,
            sitemap_lastmod: Some("2026-01-02".into()),
            ai_bots_blocked: vec!["GPTBot".into()],
            anchor_empty: 3,
            anchor_generic: 4,
            external_broken: vec![("https://x.test/".into(), "404".into())],
            word_count: Some(321),
            lang: Some("fr".into()),
            viewport: Some("width=device-width".into()),
            h2: vec!["Un".into(), "Deux".into()],
            hreflang: vec![("en".into(), "http://a.test/en/".into())],
            images: vec![
                ("a.png".into(), None),
                ("b.png".into(), Some(String::new())),
            ],
            images_total: 250,
            json_ld: vec![r#"{"@type":"Article"}"#.into()],
            nofollow_links: 3,
            extracted: vec![("ga".into(), vec!["UA-1".into(), "UA-2".into()])],
        };
        let mut store = Store::open_in_memory().unwrap();
        let id = store
            .save_run("p", &u("/"), &CrawlConfig::default(), &pages, &[])
            .unwrap();
        let (loaded, _) = store.load_run(id).unwrap();
        let f = &loaded[0].facts;
        assert_eq!(
            (f.ttfb_ms, f.total_ms, f.size),
            (Some(120), Some(340), Some(2048))
        );
        assert_eq!(f.header("x-robots-tag"), Some("noindex"));
        assert_eq!(f.content_hash.as_deref(), Some("00ff00ff00ff00ff"));
        assert_eq!(f.simhash.as_deref(), Some("0f0f0f0f0f0f0f0f"));
        assert_eq!(f.sitemap_lastmod.as_deref(), Some("2026-01-02"));
        assert_eq!(f.ai_bots_blocked, ["GPTBot"]);
        assert_eq!(
            (f.anchor_empty, f.anchor_generic, f.external_broken.len()),
            (3, 4, 1)
        );
        assert_eq!(
            (
                f.amp_html.as_deref(),
                f.is_amp,
                f.microdata,
                f.microdata_untyped
            ),
            (Some("https://e.com/amp"), true, 2, 1)
        );
        assert_eq!((f.word_count, f.lang.as_deref()), (Some(321), Some("fr")));
        assert_eq!(f.h2, ["Un", "Deux"]);
        assert_eq!(f.images, pages[0].facts.images);
        assert_eq!((f.images_total, f.nofollow_links), (250, 3));
        assert_eq!(f.hreflang, pages[0].facts.hreflang);
        assert_eq!(f.json_ld.len(), 1);
        assert_eq!(f.extracted, pages[0].facts.extracted);
        // Une page sans liste : tout reste vide, rien n'est inventé.
        assert!(loaded[1].facts.h2.is_empty() && loaded[1].facts.ttfb_ms.is_none());
    }

    #[test]
    fn a_v2_database_is_migrated_and_old_audits_stay_readable() {
        let dir = std::env::temp_dir().join(format!("callistseo-v2-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("old.db");
        let _ = std::fs::remove_file(&path);
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(MIGRATIONS[0]).unwrap();
            conn.execute_batch(MIGRATIONS[1]).unwrap();
            conn.pragma_update(None, "user_version", 2).unwrap();
            conn.execute_batch(
                "INSERT INTO projects VALUES (1, 'old', 'http://a.test/', 1);
                 INSERT INTO crawl_runs (id, project_id, started_at, settings) VALUES (1, 1, 1, '{}');
                 INSERT INTO pages (run_id, url, depth, status, is_html, in_sitemap, h1)
                   VALUES (1, 'http://a.test/', 0, 200, 1, 0, '[]');",
            )
            .unwrap();
        }
        let store = Store::open(&path).unwrap();
        let (pages, _) = store.load_run(1).unwrap();
        assert_eq!(pages.len(), 1);
        assert!(pages[0].facts.ttfb_ms.is_none() && pages[0].facts.headers.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn schedules_are_due_only_when_enabled_and_past_due() {
        let mut store = Store::open_in_memory().unwrap();
        let p = store.create_project("a", &u("/")).unwrap();
        assert!(store.schedule(p).unwrap().is_none());
        store.set_schedule(p, 60, true).unwrap();
        let sch = store.schedule(p).unwrap().unwrap();
        assert_eq!(
            (sch.interval_minutes, sch.enabled, sch.last_run_id),
            (60, true, None)
        );
        assert!(
            store.due_schedules(now()).unwrap().is_empty(),
            "pas encore échue"
        );
        let later = now() + 61 * 60;
        assert_eq!(store.due_schedules(later).unwrap().len(), 1);
        store.schedule_ran(p, 7, later).unwrap();
        let after = store.schedule(p).unwrap().unwrap();
        assert_eq!(
            (after.last_run_id, after.next_run_at),
            (Some(7), later + 3600)
        );
        assert!(store.due_schedules(later).unwrap().is_empty());
        store.set_schedule(p, 60, false).unwrap();
        assert!(
            store.due_schedules(later + 10 * 3600).unwrap().is_empty(),
            "désactivée"
        );
        assert!(!store.has_running_run(p).unwrap());
        assert!(store.delete_schedule(p).unwrap() && !store.delete_schedule(p).unwrap());
    }

    #[test]
    fn deleting_a_project_removes_everything_that_belongs_to_it_and_nothing_else() {
        let mut store = Store::open_in_memory().unwrap();
        let (pages, issues) = sample();
        let cfg = CrawlConfig::default();
        let keep = store
            .save_run("garde", &u("/"), &cfg, &pages, &issues)
            .unwrap();
        let gone = store
            .save_run("efface", &u("/"), &cfg, &pages, &issues)
            .unwrap();
        let pid = store.project_by_name("efface").unwrap().unwrap().id;
        store.set_schedule(pid, 60, true).unwrap();
        store
            .create_segment(
                pid,
                "s",
                &SegmentDef {
                    max_depth: Some(1),
                    ..Default::default()
                },
            )
            .unwrap();

        assert!(store.delete_project(pid).unwrap());
        assert!(!store.delete_project(pid).unwrap(), "déjà supprimé");
        assert!(store.project(pid).unwrap().is_none() && store.run(gone).unwrap().is_none());
        assert!(store.schedule(pid).unwrap().is_none() && store.segments(pid).unwrap().is_empty());
        let count = |sql: &str| -> i64 { store.conn.query_row(sql, [], |r| r.get(0)).unwrap() };
        // Il ne reste que les 2 pages, 1 problème et 1 audit du projet conservé : aucun orphelin.
        assert_eq!(count("SELECT COUNT(*) FROM pages"), 2);
        assert_eq!(count("SELECT COUNT(*) FROM issues"), 1);
        assert_eq!(count("SELECT COUNT(*) FROM links"), 2);
        assert_eq!(count("SELECT COUNT(*) FROM crawl_runs"), 1);
        assert_eq!(store.load_run(keep).unwrap().0.len(), 2);
    }

    /// Mesure (à lancer à la main) : `cargo test -p callistseo-storage --release perf_save_run -- --ignored --nocapture`.
    /// Trois audits d'affilée : le coût du dernier révèle l'effet de l'historique.
    #[test]
    #[ignore]
    fn perf_save_run() {
        let dir = std::env::temp_dir().join(format!("callistseo-perf-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut store = Store::open(&dir.join("perf.db")).unwrap();
        let n = 3000;
        let tpl = sample().0.remove(0);
        let pages: Vec<CrawledPage> = (0..n)
            .map(|i| {
                let mut p = tpl.clone();
                p.url = u(&format!("/p{i}"));
                p.links = (0..300)
                    .map(|k| Arc::new(u(&format!("/p{}", (i * 7 + k) % n))))
                    .collect();
                p
            })
            .collect();
        for rules in [5, 60] {
            let issues: Vec<Issue> = (0..n)
                .flat_map(|i| {
                    (0..rules).map(move |_| Issue {
                        rule: "http-error",
                        severity: Severity::Error,
                        url: u(&format!("/p{i}")),
                        message: "statut".into(),
                    })
                })
                .collect();
            let t = std::time::Instant::now();
            store
                .save_run("perf", &u("/"), &CrawlConfig::default(), &pages, &issues)
                .unwrap();
            eprintln!(
                "{n} pages, {} liens, {} issues : {:?}",
                n * 300,
                issues.len(),
                t.elapsed()
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn inlinks_of(store: &Store, run: i64) -> Vec<(String, Option<i64>, Option<i64>)> {
        let mut stmt = store
            .conn
            .prepare("SELECT url, inlinks, outlinks FROM pages WHERE run_id = ?1 ORDER BY url")
            .unwrap();
        let rows = stmt
            .query_map([run], |r| {
                Ok((
                    r.get::<_, String>(0)?.replace("http://a.test", ""),
                    r.get(1)?,
                    r.get(2)?,
                ))
            })
            .unwrap();
        rows.map(|r| r.unwrap()).collect()
    }

    /// `/` cite /a (deux fois sans effet), /a cite / et soi-même, /b cite /a et une page non crawlée.
    fn linked_pages() -> Vec<CrawledPage> {
        let mk = |path: &str, links: &[&str]| {
            let mut p = sample().0.remove(0);
            p.url = u(path);
            p.depth = usize::from(path != "/");
            p.links = links.iter().map(|l| Arc::new(u(l))).collect();
            p
        };
        vec![
            mk("/", &["/a"]),
            mk("/a", &["/", "/a"]),
            mk("/b", &["/a", "/ailleurs"]),
        ]
    }

    #[test]
    fn link_counts_are_precomputed_without_self_links_or_uncrawled_targets() {
        let mut store = Store::open_in_memory().unwrap();
        let run = store
            .save_run("p", &u("/"), &CrawlConfig::default(), &linked_pages(), &[])
            .unwrap();
        assert_eq!(
            inlinks_of(&store, run),
            [
                ("/".into(), Some(1), Some(1)),
                ("/a".into(), Some(2), Some(1)),
                ("/b".into(), Some(0), Some(1))
            ]
        );
        let stats = store.link_stats(run, 10).unwrap();
        assert_eq!(stats.internal_links, 3);
        assert_eq!(stats.top_linked[0], ("http://a.test/a".to_owned(), 2));
        assert_eq!(stats.orphans, ["http://a.test/b".to_owned()]);
    }

    #[test]
    fn a_v7_database_gets_its_link_counts_on_migration() {
        let dir = std::env::temp_dir().join(format!("callistseo-v7-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("old.db");
        let _ = std::fs::remove_file(&path);
        {
            let conn = Connection::open(&path).unwrap();
            for m in &MIGRATIONS[..7] {
                conn.execute_batch(m).unwrap();
            }
            conn.pragma_update(None, "user_version", 7).unwrap();
            conn.execute_batch(
                "INSERT INTO projects (id, name, start_url, created_at) VALUES (1, 'p', 'http://a.test/', 1);
                 INSERT INTO crawl_runs (id, project_id, started_at, settings) VALUES (1, 1, 1, '{}');
                 INSERT INTO pages (id, run_id, url, depth, status, is_html, in_sitemap, h1) VALUES
                   (1, 1, 'http://a.test/', 0, 200, 1, 0, '[]'), (2, 1, 'http://a.test/a', 1, 200, 1, 0, '[]');
                 INSERT INTO links (page_id, target) VALUES (1, 'http://a.test/a'), (2, 'http://a.test/'), (2, 'http://a.test/a');",
            )
            .unwrap();
        }
        let store = Store::open(&path).unwrap();
        assert_eq!(
            inlinks_of(&store, 1),
            [
                ("/".into(), Some(1), Some(1)),
                ("/a".into(), Some(1), Some(1))
            ]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
