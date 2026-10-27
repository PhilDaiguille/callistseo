//! API HTTP (Axum) : projets, lancement et suivi des audits, résultats paginés, exports.
//!
//! Sécurité : jeton `Authorization: Bearer`, refus d'écouter hors loopback sans jeton, limites
//! serveur sur chaque audit, et le filtre SSRF du crawler reste actif (`allow_private` n'est
//! jamais exposé).

use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{header, Request, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{any, get, post};
use axum::{Json, Router};
use callistseo_core::{normalize, CrawlConfig, CrawledPage, Issue};
use callistseo_crawler::CrawlStats;
use callistseo_seo_rules::{analyze, rule_info, RulesConfig};
use callistseo_storage::{IssueQuery, PageQuery, RunInfo, Store};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};
use tokio::task::AbortHandle;
use url::Url;

#[derive(Debug, Clone)]
pub struct ApiConfig {
    /// Jeton Bearer attendu ; `None` uniquement autorisé sur loopback.
    pub token: Option<String>,
    /// Plafonds appliqués à chaque audit demandé.
    pub max_pages: usize,
    pub max_depth: usize,
    pub max_concurrency: usize,
    /// Audits simultanés.
    pub max_running: usize,
    /// Identifiant du robot pour tous les audits. Défini par l'opérateur : un client de l'API ne
    /// peut pas le choisir (il ne pourrait pas faire passer le robot pour un navigateur).
    pub user_agent: String,
    /// Délai minimal entre deux requêtes d'un audit (ms) : un client ne peut pas descendre en dessous.
    pub min_delay_ms: u64,
    /// Dossier de l'interface construite (`apps/web/dist`), servie sur `/` avec repli vers
    /// `index.html` pour le routage côté client. Les fichiers statiques sont publics ; `/api/…`
    /// reste protégé par le jeton.
    pub static_dir: Option<PathBuf>,
    /// Derrière un proxy (HTTPS) : l'adresse du client est la dernière de `X-Forwarded-For`
    /// (celle qu'ajoute le proxy). Sans proxy devant, ne pas l'activer : l'en-tête serait falsifiable.
    pub trust_proxy: bool,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            token: None,
            max_pages: 10_000,
            max_depth: 20,
            max_concurrency: 16,
            max_running: 2,
            user_agent: callistseo_core::default_user_agent(),
            min_delay_ms: 0,
            static_dir: None,
            trust_proxy: false,
        }
    }
}

type Running = HashMap<i64, (AbortHandle, Arc<CrawlStats>)>;

#[derive(Clone)]
pub struct AppState {
    store: Arc<Mutex<Store>>,
    cfg: Arc<ApiConfig>,
    running: Arc<Mutex<Running>>,
    auth_fails: Arc<AuthLimiter>,
}

/// Échecs d'authentification par adresse : au-delà de `AUTH_FAILS` dans la fenêtre, l'adresse reçoit
/// des 429 (sans comparaison du jeton) jusqu'à la fin de la fenêtre.
#[derive(Default)]
struct AuthLimiter(Mutex<HashMap<IpAddr, (Instant, u32)>>);

const AUTH_FAILS: u32 = 10;
const AUTH_WINDOW: Duration = Duration::from_secs(60);

impl AuthLimiter {
    /// Secondes d'attente restantes si `ip` est bloquée.
    fn blocked(&self, ip: IpAddr, now: Instant) -> Option<u64> {
        let map = lock(&self.0);
        let (start, n) = map.get(&ip)?;
        let elapsed = now.duration_since(*start);
        (*n >= AUTH_FAILS && elapsed < AUTH_WINDOW)
            .then(|| (AUTH_WINDOW - elapsed).as_secs().max(1))
    }

    fn fail(&self, ip: IpAddr, now: Instant) {
        let mut map = lock(&self.0);
        // ponytail: purge linéaire au-delà de 10 000 adresses ; suffisant pour un serveur d'équipe.
        if map.len() > 10_000 {
            map.retain(|_, (start, _)| now.duration_since(*start) < AUTH_WINDOW);
        }
        let entry = map.entry(ip).or_insert((now, 0));
        if now.duration_since(entry.0) >= AUTH_WINDOW {
            *entry = (now, 0);
        }
        entry.1 += 1;
    }
}

/// Adresse du client : pair TCP, ou dernière entrée de `X-Forwarded-For` si `trust_proxy`.
/// Sans information (appel en processus, tests), l'adresse non spécifiée sert de clé commune.
fn client_ip(cfg: &ApiConfig, req: &Request<Body>) -> IpAddr {
    let forwarded = || {
        req.headers()
            .get("x-forwarded-for")?
            .to_str()
            .ok()?
            .rsplit(',')
            .next()?
            .trim()
            .parse()
            .ok()
    };
    let peer = || {
        req.extensions()
            .get::<axum::extract::ConnectInfo<SocketAddr>>()
            .map(|c| c.0.ip())
    };
    cfg.trust_proxy
        .then(forwarded)
        .flatten()
        .or_else(peer)
        .unwrap_or(IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED))
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl AppState {
    pub fn new(store: Store, cfg: ApiConfig) -> Self {
        Self {
            store: Arc::new(Mutex::new(store)),
            cfg: Arc::new(cfg),
            running: Arc::default(),
            auth_fails: Arc::default(),
        }
    }

    /// Exécute une opération SQLite hors du runtime asynchrone.
    async fn db<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Store) -> Result<T, String> + Send + 'static,
    ) -> Result<T, ApiError> {
        let store = self.store.clone();
        tokio::task::spawn_blocking(move || f(&mut lock(&store)))
            .await
            .map_err(|e| ApiError::internal(e.to_string()))?
            .map_err(ApiError::internal)
    }

    async fn require_run(&self, id: i64) -> Result<RunInfo, ApiError> {
        self.db(move |s| s.run(id))
            .await?
            .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "audit introuvable"))
    }
}

pub struct ApiError(StatusCode, String);

impl ApiError {
    fn new(status: StatusCode, msg: impl Into<String>) -> Self {
        Self(status, msg.into())
    }

    /// Le détail reste dans les logs : il ne part pas chez le client.
    fn internal(detail: String) -> Self {
        eprintln!("erreur interne: {detail}");
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "erreur interne")
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({ "error": self.1 }))).into_response()
    }
}

type ApiResult<T> = Result<T, ApiError>;

/// Comparaison en temps constant (hors longueur) pour ne pas révéler le jeton par le timing.
fn token_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

async fn auth(State(s): State<AppState>, req: Request<Body>, next: Next) -> Response {
    if let Some(token) = &s.cfg.token {
        let (ip, now) = (client_ip(&s.cfg, &req), Instant::now());
        if let Some(wait) = s.auth_fails.blocked(ip, now) {
            let mut resp = ApiError::new(
                StatusCode::TOO_MANY_REQUESTS,
                "trop de tentatives, réessayez plus tard",
            )
            .into_response();
            resp.headers_mut()
                .insert(header::RETRY_AFTER, header::HeaderValue::from(wait));
            return resp;
        }
        let given = req
            .headers()
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "));
        if !given.is_some_and(|g| token_eq(g.as_bytes(), token.as_bytes())) {
            s.auth_fails.fail(ip, now);
            return ApiError::new(StatusCode::UNAUTHORIZED, "jeton manquant ou invalide")
                .into_response();
        }
    }
    next.run(req).await
}

/// Réponse d'un appel en processus (voir `LocalApi`).
#[derive(Debug, Serialize)]
pub struct InProcessResponse {
    pub status: u16,
    pub content_type: Option<String>,
    pub content_disposition: Option<String>,
    pub body: String,
}

/// L'API sans réseau : l'application desktop appelle le routeur directement, sans port ouvert
/// ni jeton (rien n'écoute, donc rien à protéger côté réseau).
#[derive(Clone)]
pub struct LocalApi {
    router: Router,
    state: AppState,
}

/// Plafond de la réponse d'un appel en processus.
const LOCAL_BODY_MAX: usize = 256 * 1024 * 1024;

impl LocalApi {
    pub fn new(store: Store, cfg: ApiConfig) -> Self {
        let state = AppState::new(store, cfg);
        Self {
            router: router(state.clone()),
            state,
        }
    }

    /// Planificateur des audits récurrents (à lancer une fois, ex. `tauri::async_runtime::spawn`).
    pub fn scheduler(&self) -> Scheduler {
        Scheduler(self.state.clone())
    }

    /// Seules les routes `/api/…` en GET/POST/PUT/DELETE sont joignables ; le corps est du JSON.
    pub async fn call(
        &self,
        method: &str,
        path: &str,
        body: Option<String>,
    ) -> Result<InProcessResponse, String> {
        use axum::http::Method;
        use tower::ServiceExt;

        if !path.starts_with("/api/") {
            return Err("chemin refusé".into());
        }
        let method = match method {
            "GET" => Method::GET,
            "POST" => Method::POST,
            "PUT" => Method::PUT,
            "DELETE" => Method::DELETE,
            other => return Err(format!("méthode refusée : {other}")),
        };
        let mut req = Request::builder().method(method).uri(path);
        if body.is_some() {
            req = req.header(header::CONTENT_TYPE, "application/json");
        }
        let req = req
            .body(Body::from(body.unwrap_or_default()))
            .map_err(|e| e.to_string())?;
        let resp = match self.router.clone().oneshot(req).await {
            Ok(r) => r,
            Err(never) => match never {},
        };
        let status = resp.status().as_u16();
        // Fonction (et non closure) : aucune référence à `resp` ne reste vivante pendant l'`await`
        // ci-dessous, ce qui rend le futur `Send` pour Tauri.
        fn text(resp: &Response, name: header::HeaderName) -> Option<String> {
            resp.headers()
                .get(name)
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned)
        }
        let (content_type, content_disposition) = (
            text(&resp, header::CONTENT_TYPE),
            text(&resp, header::CONTENT_DISPOSITION),
        );
        let bytes = axum::body::to_bytes(resp.into_body(), LOCAL_BODY_MAX)
            .await
            .map_err(|e| e.to_string())?;
        Ok(InProcessResponse {
            status,
            content_type,
            content_disposition,
            body: String::from_utf8_lossy(&bytes).into_owned(),
        })
    }
}

async fn health() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

/// Une route `/api/…` inconnue ne doit jamais retomber sur l'interface (`index.html`).
async fn api_not_found() -> ApiError {
    ApiError::new(StatusCode::NOT_FOUND, "route inconnue")
}

pub fn router(state: AppState) -> Router {
    let static_dir = state.cfg.static_dir.clone();
    let app = Router::new()
        .route("/api/projects", post(create_project).get(list_projects))
        .route("/api/projects/{id}", get(get_project))
        .route("/api/projects/{id}/crawls", post(start_crawl))
        .route("/api/crawls/{id}", get(get_crawl))
        .route("/api/crawls/{id}/pages", get(crawl_pages))
        .route("/api/crawls/{id}/issues", get(crawl_issues))
        .route("/api/crawls/{id}/graph", get(crawl_graph))
        .route("/api/projects/{id}", axum::routing::delete(delete_project))
        .route("/api/projects/{id}/config", get(get_config).put(put_config))
        .route(
            "/api/projects/{id}/schedule",
            get(get_schedule).put(put_schedule).delete(delete_schedule),
        )
        .route("/api/crawls/{id}/cancel", post(cancel_crawl))
        .route(
            "/api/crawls/{id}/pause",
            post(|s, id| control_crawl(s, id, Control::Pause)),
        )
        .route(
            "/api/crawls/{id}/resume",
            post(|s, id| control_crawl(s, id, Control::Resume)),
        )
        .route(
            "/api/crawls/{id}/stop",
            post(|s, id| control_crawl(s, id, Control::Stop)),
        )
        .route("/api/crawls/{id}/reports", get(crawl_reports))
        .route("/api/crawls/{id}/segments", get(crawl_segments))
        .route(
            "/api/projects/{id}/segments",
            get(list_segments).post(create_segment),
        )
        .route("/api/segments/{id}", axum::routing::delete(delete_segment))
        .route("/api/crawls/{id}/rows", get(crawl_rows))
        .route("/api/crawls/{id}/export", get(export_crawl))
        .route("/api/crawls/{id}/sitemap.xml", get(sitemap_crawl))
        .route("/api/crawls/{before}/diff/{after}", get(diff_crawls))
        .route("/api/crawls/{id}/page", get(page_detail))
        .route("/api/crawls/{id}/links", get(links_overview))
        .route("/api/{*rest}", any(api_not_found))
        .layer(middleware::from_fn_with_state(state.clone(), auth))
        .with_state(state)
        // Ajoutée après la couche d'authentification : publique (sonde de santé des conteneurs).
        .route("/api/health", get(health));
    match static_dir {
        Some(dir) => app.fallback_service(
            tower_http::services::ServeDir::new(&dir)
                .fallback(tower_http::services::ServeFile::new(dir.join("index.html"))),
        ),
        None => app,
    }
}

/// Le routeur tel que servi en HTTP : compression gzip/brotli selon `Accept-Encoding` (JSON volumineux
/// et fichiers de l'interface : le JS passe de ≈ 475 Ko à ≈ 140 Ko) et en-têtes de cache. Les appels en
/// processus (`LocalApi`) n'ont pas besoin de compresser et utilisent `router` directement.
///
/// BREACH : la compression peut trahir un secret réfléchi à côté d'une valeur choisie par un attaquant,
/// mais le jeton voyage dans un en-tête de requête (pas dans le corps ni dans un cookie) et aucune
/// réponse ne le contient.
pub fn http_app(state: AppState) -> Router {
    router(state.clone())
        .layer(middleware::from_fn(cache_headers))
        .layer(middleware::from_fn_with_state(state, access_log))
        .layer(
            tower_http::compression::CompressionLayer::new()
                .gzip(true)
                .br(true),
        )
}

/// Fichiers de l'interface : `/assets/*` porte un hash dans son nom (immuable, un an) ; la page
/// d'entrée est revalidée à chaque visite (elle désigne les bons fichiers après une mise à jour).
/// Réponses de l'API : jamais mises en cache (données privées).
async fn cache_headers(req: Request<Body>, next: middleware::Next) -> Response {
    let path = req.uri().path().to_owned();
    let mut resp = next.run(req).await;
    // Un fichier absent de `/assets/` retombe sur `index.html` (repli de l'interface) avec un 200 :
    // il ne doit surtout pas être mis en cache pour un an, d'où le test du type de contenu.
    let is_html = resp
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|t| t.starts_with("text/html"));
    let value = if path.starts_with("/api/") {
        "no-store"
    } else if path.starts_with("/assets/") && resp.status().is_success() && !is_html {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };
    let h = resp.headers_mut();
    h.insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static(value),
    );
    // Interface sans script ni ressource externe (polices embarquées) : tout vient de l'origine.
    // `style-src 'unsafe-inline'` : des composants posent des styles en ligne (positionnement).
    h.insert(
        header::CONTENT_SECURITY_POLICY,
        header::HeaderValue::from_static(
            "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; \
             img-src 'self' data:; object-src 'none'; base-uri 'none'; form-action 'self'; \
             frame-ancestors 'none'",
        ),
    );
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        header::HeaderValue::from_static("nosniff"),
    );
    h.insert(
        header::REFERRER_POLICY,
        header::HeaderValue::from_static("no-referrer"),
    );
    resp
}

/// Journal d'accès sur stderr (`docker compose logs`) : adresse, méthode, chemin sans la requête
/// (filtres et recherches restent privés), statut, durée. La sonde de santé n'y figure pas.
async fn access_log(State(s): State<AppState>, req: Request<Body>, next: Next) -> Response {
    let path = req.uri().path().to_owned();
    if path == "/api/health" {
        return next.run(req).await;
    }
    let (ip, method, start) = (
        client_ip(&s.cfg, &req),
        req.method().clone(),
        Instant::now(),
    );
    let resp = next.run(req).await;
    eprintln!(
        "{ip} {method} {path} {} {}ms",
        resp.status().as_u16(),
        start.elapsed().as_millis()
    );
    resp
}

/// Démarre le serveur (bloquant jusqu'à Ctrl-C).
pub async fn serve(addr: SocketAddr, mut store: Store, cfg: ApiConfig) -> Result<(), String> {
    callistseo_crawler::validate_user_agent(&cfg.user_agent)?;
    match &cfg.token {
        Some(t) if t.len() < 16 => return Err("le jeton doit faire au moins 16 caractères".into()),
        None if !addr.ip().is_loopback() => {
            return Err("écouter hors de 127.0.0.1/::1 exige un jeton (--token)".into())
        }
        _ => {}
    }
    if let Some(dir) = &cfg.static_dir {
        if !dir.join("index.html").is_file() {
            return Err(format!(
                "interface introuvable : {} ne contient pas index.html",
                dir.display()
            ));
        }
    }
    let stale = store.fail_stale_runs()?;
    if stale > 0 {
        eprintln!("{stale} audit(s) interrompu(s) marqué(s) en échec");
    }
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| format!("impossible d'écouter sur {addr}: {e}"))?;
    eprintln!("API sur http://{addr} (Ctrl-C pour arrêter)");
    let state = AppState::new(store, cfg);
    tokio::spawn(Scheduler(state.clone()).run());
    axum::serve(
        listener,
        http_app(state).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await
    .map_err(|e| e.to_string())
}

/// Ctrl-C (SIGINT) ou SIGTERM : c'est ce que `docker stop` envoie, et en tant que PID 1 le
/// processus l'ignorerait sans gestionnaire (arrêt brutal après 10 s).
async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut term = signal(SignalKind::terminate()).ok();
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = async {
                match term.as_mut() {
                    Some(t) => { t.recv().await; }
                    None => std::future::pending::<()>().await,
                }
            } => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

// ---- projets -----------------------------------------------------------------------------

#[derive(Deserialize)]
struct NewProject {
    name: String,
    start_url: String,
}

fn project_json(p: &callistseo_storage::Project) -> Value {
    json!({ "id": p.id, "name": p.name, "start_url": p.start_url, "created_at": p.created_at })
}

async fn create_project(
    State(s): State<AppState>,
    Json(body): Json<NewProject>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let name = body.name.trim().to_owned();
    if name.is_empty() || name.chars().count() > 100 {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "le nom doit faire entre 1 et 100 caractères",
        ));
    }
    let start = normalize(body.start_url.trim(), None).ok_or_else(|| {
        ApiError::new(StatusCode::BAD_REQUEST, "URL invalide (http/https attendu)")
    })?;
    let project = s
        .db(move |st| {
            if st.project_by_name(&name)?.is_some() {
                return Ok(None);
            }
            let id = st.create_project(&name, &start)?;
            st.project(id)
        })
        .await?
        .ok_or_else(|| ApiError::new(StatusCode::CONFLICT, "un projet porte déjà ce nom"))?;
    Ok((StatusCode::CREATED, Json(project_json(&project))))
}

async fn list_projects(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    let (projects, latest) = s.db(|st| Ok((st.projects()?, st.latest_runs()?))).await?;
    let items: Vec<Value> = projects
        .iter()
        .map(|p| {
            let mut out = project_json(p);
            // Dernier audit du projet (état et compteurs), pour la liste d'accueil.
            out["last_crawl"] = latest
                .iter()
                .find(|r| r.project_id == p.id)
                .map_or(Value::Null, |r| run_json(r, None));
            out
        })
        .collect();
    Ok(Json(json!({ "projects": items })))
}

/// Supprime un projet et tout ce qui en dépend. Refusé (409) tant qu'un de ses audits tourne : il
/// faut l'annuler ou l'arrêter d'abord, sinon son résultat serait écrit pour un projet disparu.
async fn delete_project(State(s): State<AppState>, Path(id): Path<i64>) -> ApiResult<StatusCode> {
    let outcome = s
        .db(move |st| {
            if st.project(id)?.is_none() {
                return Ok(None);
            }
            if st.has_running_run(id)? {
                return Ok(Some(false));
            }
            st.delete_project(id).map(Some)
        })
        .await?;
    match outcome {
        None => Err(ApiError::new(StatusCode::NOT_FOUND, "projet introuvable")),
        Some(false) => Err(ApiError::new(
            StatusCode::CONFLICT,
            "un audit de ce projet est en cours : arrêtez-le ou annulez-le avant de supprimer le projet",
        )),
        Some(true) => Ok(StatusCode::NO_CONTENT),
    }
}

async fn get_project(State(s): State<AppState>, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    let (project, runs) = s
        .db(move |st| Ok((st.project(id)?, st.runs_of_project(id)?)))
        .await?;
    let project =
        project.ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "projet introuvable"))?;
    let mut out = project_json(&project);
    out["crawls"] = runs.iter().map(|r| run_json(r, None)).collect();
    Ok(Json(out))
}

// ---- audits ------------------------------------------------------------------------------

/// `live` : statistiques de l'audit en cours (absentes d'un audit terminé).
fn run_json(r: &RunInfo, live: Option<&CrawlStats>) -> Value {
    let pages = live.map_or(r.pages, |l| l.completed.load(Ordering::Relaxed) as i64);
    let mut out = json!({
        "id": r.id, "project_id": r.project_id, "project": r.project, "start_url": r.start_url,
        "status": r.status, "started_at": r.started_at, "finished_at": r.finished_at,
        "error": r.error, "pages": pages,
        "errors": r.errors, "warnings": r.warnings,
    });
    if let Some(l) = live {
        out["live"] = json!({
            "in_flight": l.in_flight.load(Ordering::Relaxed),
            "concurrency": l.limit.load(Ordering::Relaxed),
            "queued": l.queued.load(Ordering::Relaxed),
            "max_pages": l.max_pages.load(Ordering::Relaxed),
            "urls_per_second": (l.urls_per_second() * 10.0).round() / 10.0,
            "avg_latency_ms": l.avg_latency().as_millis() as u64,
            "failures": l.failures.load(Ordering::Relaxed),
            "paused": l.paused.load(Ordering::Relaxed),
            "stopping": l.stop.load(Ordering::Relaxed),
        });
    }
    out
}

/// Configuration d'audit d'un projet. Tout champ absent garde sa valeur par défaut ; les limites du
/// serveur (`ApiConfig`) s'appliquent toujours par-dessus.
#[derive(Serialize, Deserialize, Default, Clone, Debug)]
#[serde(default, deny_unknown_fields)]
struct ProjectConfig {
    include: Vec<String>,
    exclude: Vec<String>,
    strip_params: Vec<String>,
    max_url_length: Option<usize>,
    /// robots.txt à utiliser à la place de celui du site.
    robots_txt: Option<String>,
    max_pages: Option<usize>,
    max_depth: Option<usize>,
    delay_ms: Option<u64>,
    concurrency: Option<usize>,
    /// Extractions personnalisées et recherches dans le code source.
    extractors: Vec<ExtractorDef>,
    /// Vérifie les liens vers d'autres sites (requêtes vers des tiers, 500 URL au plus).
    check_external: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
struct ExtractorDef {
    name: String,
    /// `regex`, `css` ou `contains`.
    kind: String,
    pattern: String,
    #[serde(default)]
    attr: Option<String>,
}

impl ExtractorDef {
    fn to_core(&self) -> Result<callistseo_core::Extractor, String> {
        use callistseo_core::ExtractKind;
        let kind = match self.kind.as_str() {
            "regex" => ExtractKind::Regex,
            "css" => ExtractKind::Css,
            "contains" => ExtractKind::Contains,
            other => {
                return Err(format!(
                    "extraction « {} » : type inconnu « {other} »",
                    self.name
                ))
            }
        };
        Ok(callistseo_core::Extractor {
            name: self.name.clone(),
            kind,
            pattern: self.pattern.clone(),
            attr: self.attr.clone(),
        })
    }
}

const MAX_ROBOTS_OVERRIDE: usize = 64 * 1024;

impl ProjectConfig {
    fn scope(&self) -> callistseo_core::UrlScope {
        callistseo_core::UrlScope {
            include: self.include.clone(),
            exclude: self.exclude.clone(),
            strip_params: self.strip_params.clone(),
            max_url_length: self.max_url_length,
            robots_override: self.robots_txt.clone().filter(|t| !t.trim().is_empty()),
        }
    }

    fn validate(&self) -> Result<(), String> {
        if self
            .robots_txt
            .as_ref()
            .is_some_and(|t| t.len() > MAX_ROBOTS_OVERRIDE)
        {
            return Err("robots.txt personnalisé : 64 Ko au plus".into());
        }
        if self
            .max_url_length
            .is_some_and(|n| !(10..=10_000).contains(&n))
        {
            return Err("longueur d'URL maximale : entre 10 et 10 000".into());
        }
        callistseo_crawler::Scope::new(&self.scope())?;
        callistseo_crawler::Extractors::new(&self.core_extractors()?).map(drop)
    }

    fn core_extractors(&self) -> Result<Vec<callistseo_core::Extractor>, String> {
        self.extractors.iter().map(ExtractorDef::to_core).collect()
    }
}

async fn stored_config(s: &AppState, project_id: i64) -> ApiResult<ProjectConfig> {
    let raw = s
        .db(move |st| {
            st.project(project_id)?
                .map(|_| st.project_config(project_id))
                .transpose()
        })
        .await?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "projet introuvable"))?;
    // Une configuration illisible (écrite par une autre version) retombe sur les valeurs par défaut.
    Ok(raw
        .and_then(|j| serde_json::from_str(&j).ok())
        .unwrap_or_default())
}

async fn get_config(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> ApiResult<Json<ProjectConfig>> {
    Ok(Json(stored_config(&s, id).await?))
}

async fn put_config(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(cfg): Json<ProjectConfig>,
) -> ApiResult<Json<ProjectConfig>> {
    cfg.validate()
        .map_err(|e| ApiError::new(StatusCode::BAD_REQUEST, e))?;
    let json = serde_json::to_string(&cfg).map_err(|e| ApiError::internal(e.to_string()))?;
    if !s.db(move |st| st.set_project_config(id, &json)).await? {
        return Err(ApiError::new(StatusCode::NOT_FOUND, "projet introuvable"));
    }
    Ok(Json(cfg))
}

#[derive(Deserialize, Default)]
struct CrawlRequest {
    /// Délai minimal entre deux requêtes (ms), borné par le serveur.
    delay_ms: Option<u64>,
    max_pages: Option<usize>,
    max_depth: Option<usize>,
    concurrency: Option<usize>,
}

/// Délai demandé, relevé au minimum du serveur et plafonné à 30 s.
fn clamp_delay(requested: Option<u64>, min_ms: u64) -> std::time::Duration {
    const MAX_MS: u64 = 30_000;
    let ms = requested.unwrap_or(0).max(min_ms).min(MAX_MS);
    std::time::Duration::from_millis(ms)
}

async fn start_crawl(
    State(s): State<AppState>,
    Path(project_id): Path<i64>,
    body: Option<Json<CrawlRequest>>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let run_id = launch_crawl(&s, project_id, body.map(|Json(b)| b).unwrap_or_default()).await?;
    let run = s.require_run(run_id).await?;
    Ok((StatusCode::ACCEPTED, Json(run_json(&run, None))))
}

/// Crée l'audit et le lance en tâche de fond ; partagé par la demande manuelle et la planification.
async fn launch_crawl(s: &AppState, project_id: i64, req: CrawlRequest) -> ApiResult<i64> {
    let project = s
        .db(move |st| st.project(project_id))
        .await?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "projet introuvable"))?;
    let start = normalize(&project.start_url, None).ok_or_else(|| {
        ApiError::internal(format!("URL de projet invalide: {}", project.start_url))
    })?;

    let limits = &s.cfg;
    // Priorité : demande de l'audit > configuration du projet > défauts du serveur (qui plafonnent tout).
    let project_cfg = stored_config(s, project_id).await?;
    let req = CrawlRequest {
        max_pages: req.max_pages.or(project_cfg.max_pages),
        max_depth: req.max_depth.or(project_cfg.max_depth),
        concurrency: req.concurrency.or(project_cfg.concurrency),
        delay_ms: req.delay_ms.or(project_cfg.delay_ms),
    };
    let cfg = CrawlConfig {
        scope: project_cfg.scope(),
        extractors: project_cfg.core_extractors().map_err(ApiError::internal)?,
        check_external: project_cfg.check_external,
        max_pages: req
            .max_pages
            .unwrap_or(limits.max_pages)
            .clamp(1, limits.max_pages),
        max_depth: req
            .max_depth
            .unwrap_or(limits.max_depth)
            .min(limits.max_depth),
        // Concurrence demandée = fixe ; sinon adaptative de 8 jusqu'au plafond du serveur.
        concurrency: req
            .concurrency
            .unwrap_or(limits.max_concurrency.min(8))
            .clamp(1, limits.max_concurrency),
        max_concurrency: req.concurrency.map_or(limits.max_concurrency, |c| {
            c.clamp(1, limits.max_concurrency)
        }),
        delay: clamp_delay(req.delay_ms, limits.min_delay_ms),
        user_agent: limits.user_agent.clone(),
        ..CrawlConfig::default() // allow_private = false, robots.txt respecté
    };

    if lock(&s.running).len() >= limits.max_running {
        return Err(ApiError::new(
            StatusCode::TOO_MANY_REQUESTS,
            "trop d'audits en cours, réessayez plus tard",
        ));
    }
    let run_cfg = cfg.clone();
    let run_id = s.db(move |st| st.create_run(project_id, &run_cfg)).await?;

    spawn_crawl(s, run_id, start, cfg);
    Ok(run_id)
}

// ---- planification -----------------------------------------------------------------------------

/// Intervalle minimal : une planification ne doit pas devenir un robot qui martèle un site.
const MIN_INTERVAL_MINUTES: i64 = 60;
const MAX_INTERVAL_MINUTES: i64 = 60 * 24 * 90;
const TICK: std::time::Duration = std::time::Duration::from_secs(30);

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ScheduleBody {
    interval_minutes: i64,
    #[serde(default = "yes")]
    enabled: bool,
}

fn yes() -> bool {
    true
}

fn schedule_json(sch: &callistseo_storage::Schedule) -> Value {
    json!({
        "interval_minutes": sch.interval_minutes, "enabled": sch.enabled,
        "next_run_at": sch.next_run_at, "last_run_id": sch.last_run_id,
    })
}

async fn get_schedule(State(s): State<AppState>, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    let sch = s
        .db(move |st| st.project(id)?.map(|_| st.schedule(id)).transpose())
        .await?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "projet introuvable"))?;
    Ok(Json(sch.as_ref().map_or(Value::Null, schedule_json)))
}

async fn put_schedule(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(b): Json<ScheduleBody>,
) -> ApiResult<Json<Value>> {
    if !(MIN_INTERVAL_MINUTES..=MAX_INTERVAL_MINUTES).contains(&b.interval_minutes) {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            format!(
                "intervalle : de {MIN_INTERVAL_MINUTES} minutes à {MAX_INTERVAL_MINUTES} minutes"
            ),
        ));
    }
    let sch = s
        .db(move |st| {
            if st.project(id)?.is_none() {
                return Ok(None);
            }
            st.set_schedule(id, b.interval_minutes, b.enabled)?;
            st.schedule(id)
        })
        .await?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "projet introuvable"))?;
    Ok(Json(schedule_json(&sch)))
}

async fn delete_schedule(State(s): State<AppState>, Path(id): Path<i64>) -> ApiResult<StatusCode> {
    if s.db(move |st| st.delete_schedule(id)).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::new(StatusCode::NOT_FOUND, "aucune planification"))
    }
}

/// Lance les audits dont l'échéance est passée. Un projet déjà en cours d'audit est laissé de côté
/// (réessayé au prochain passage) ; si le serveur est saturé, on réessaie aussi plus tard.
async fn run_due_schedules(s: &AppState, now: i64) {
    let Ok(due) = s.db(move |st| st.due_schedules(now)).await else {
        return;
    };
    for sch in due {
        let project_id = sch.project_id;
        if s.db(move |st| st.has_running_run(project_id))
            .await
            .unwrap_or(true)
        {
            continue;
        }
        match launch_crawl(s, project_id, CrawlRequest::default()).await {
            Ok(run_id) => {
                let _ = s
                    .db(move |st| st.schedule_ran(project_id, run_id, now))
                    .await;
                eprintln!("audit planifié #{run_id} lancé (projet {project_id})");
            }
            Err(e) => eprintln!("audit planifié du projet {project_id} non lancé : {}", e.1),
        }
    }
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// Tâche de fond de la planification : à lancer une seule fois, dans un runtime tokio.
#[derive(Clone)]
pub struct Scheduler(AppState);

impl Scheduler {
    pub async fn run(self) {
        let mut tick = tokio::time::interval(TICK);
        loop {
            tick.tick().await;
            let now = now_secs();
            run_due_schedules(&self.0, now).await;
        }
    }
}

/// Lance l'audit en tâche de fond. Fonction synchrone : aucun verrou n'est tenu à travers un
/// `await`, et le verrou de `running` reste pris jusqu'à l'insertion, donc la tâche (qui se retire
/// elle-même à la fin) ne peut pas passer devant, même si elle termine instantanément.
fn spawn_crawl(s: &AppState, run_id: i64, start: Url, cfg: CrawlConfig) {
    let stats = Arc::new(CrawlStats::default());
    let mut running = lock(&s.running);
    let task_state = s.clone();
    let task_stats = stats.clone();
    let handle = tokio::spawn(async move {
        let outcome = match callistseo_crawler::crawl_observed(start, cfg, task_stats).await {
            Ok(mut pages) => {
                pages.sort_by(|a, b| (a.depth, &a.url).cmp(&(b.depth, &b.url)));
                let issues = analyze(&pages, &RulesConfig::default());
                task_state
                    .db(move |st| st.finish_run(run_id, &pages, &issues))
                    .await
            }
            Err(e) => {
                task_state
                    .db(move |st| st.set_run_status(run_id, "failed", Some(&e)).map(drop))
                    .await
            }
        };
        if let Err(e) = outcome {
            eprintln!("audit #{run_id}: résultat non enregistré ({})", e.1);
        }
        lock(&task_state.running).remove(&run_id);
    });
    running.insert(run_id, (handle.abort_handle(), stats));
}

async fn get_crawl(State(s): State<AppState>, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    let run = s.require_run(id).await?;
    let live = lock(&s.running).get(&id).map(|(_, stats)| stats.clone());
    Ok(Json(run_json(&run, live.as_deref())))
}

async fn cancel_crawl(State(s): State<AppState>, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    let run = s.require_run(id).await?;
    if run.status != "running" {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            format!("audit déjà {}", run.status),
        ));
    }
    let handle = lock(&s.running).remove(&id);
    if let Some((handle, _)) = handle {
        handle.abort(); // les requêtes en vol sont abandonnées avec la tâche
    }
    if !s
        .db(move |st| st.set_run_status(id, "cancelled", None))
        .await?
    {
        return Err(ApiError::new(StatusCode::CONFLICT, "audit déjà terminé"));
    }
    Ok(Json(run_json(&s.require_run(id).await?, None)))
}

enum Control {
    Pause,
    Resume,
    Stop,
}

/// Pause / reprise / arrêt d'un audit en cours. L'arrêt est propre : les requêtes en vol finissent,
/// puis les pages déjà explorées sont analysées et enregistrées (contrairement à « annuler »).
async fn control_crawl(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    action: Control,
) -> ApiResult<Json<Value>> {
    let run = s.require_run(id).await?;
    let stats = lock(&s.running).get(&id).map(|(_, st)| st.clone());
    let Some(stats) = stats.filter(|_| run.status == "running") else {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            format!("audit déjà {}", run.status),
        ));
    };
    match action {
        Control::Pause => stats.paused.store(true, Ordering::Relaxed),
        Control::Resume => stats.paused.store(false, Ordering::Relaxed),
        Control::Stop => {
            stats.stop.store(true, Ordering::Relaxed);
            stats.paused.store(false, Ordering::Relaxed);
        }
    }
    Ok(Json(run_json(&run, Some(&stats))))
}

// ---- résultats ---------------------------------------------------------------------------

// ---- segments ----------------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SegmentBody {
    name: String,
    definition: callistseo_storage::SegmentDef,
}

fn segment_json(id: i64, name: &str, def: &callistseo_storage::SegmentDef) -> Value {
    json!({"id": id, "name": name, "definition": def})
}

async fn list_segments(State(s): State<AppState>, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    let list = s
        .db(move |st| st.project(id)?.map(|_| st.segments(id)).transpose())
        .await?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "projet introuvable"))?;
    Ok(Json(
        json!({"segments": list.iter().map(|(i, n, d)| segment_json(*i, n, d)).collect::<Vec<_>>()}),
    ))
}

async fn create_segment(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(b): Json<SegmentBody>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let bad = |m: String| ApiError::new(StatusCode::BAD_REQUEST, m);
    let name = b.name.trim().to_owned();
    if !(1..=60).contains(&name.chars().count()) {
        return Err(bad("nom de 1 à 60 caractères".into()));
    }
    if b.definition.is_empty() {
        return Err(bad("segment : renseignez au moins un critère".into()));
    }
    b.definition.validate().map_err(bad)?;
    let (n, def) = (name.clone(), b.definition.clone());
    // Les refus métier (doublon, plafond) sortent de la fermeture : `db` masque les vraies pannes.
    let created = s
        .db(move |st| {
            if st.project(id)?.is_none() {
                return Ok(None);
            }
            Ok(Some(st.create_segment(id, &n, &def)))
        })
        .await?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "projet introuvable"))?
        .map_err(|e| ApiError::new(StatusCode::CONFLICT, e))?;
    Ok((
        StatusCode::CREATED,
        Json(segment_json(created, &name, &b.definition)),
    ))
}

async fn delete_segment(State(s): State<AppState>, Path(id): Path<i64>) -> ApiResult<StatusCode> {
    if s.db(move |st| st.delete_segment(id)).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::new(StatusCode::NOT_FOUND, "segment introuvable"))
    }
}

/// Segments du projet de l'audit, avec le nombre d'URL de cet audit dans chacun.
async fn crawl_segments(State(s): State<AppState>, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    let run = s.require_run(id).await?;
    let project_id = run.project_id;
    let counted = s
        .db(move |st| {
            st.segments(project_id)?
                .into_iter()
                .map(|(sid, name, def)| Ok((sid, name, st.segment_count(id, &def)?)))
                .collect::<Result<Vec<_>, String>>()
        })
        .await?;
    Ok(Json(
        json!({"segments": counted.iter().map(|(i, n, c)| json!({"id": i, "name": n, "count": c})).collect::<Vec<_>>()}),
    ))
}

fn column_json(c: &callistseo_storage::Column) -> Value {
    json!({"id": c.id, "label": c.label, "numeric": c.numeric})
}

/// Rapports (groupés) avec leur nombre d'URL, et problèmes par règle : de quoi bâtir la barre latérale.
async fn crawl_reports(State(s): State<AppState>, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    s.require_run(id).await?;
    let (counts, summary) = s
        .db(move |st| Ok((st.report_counts(id)?, st.issue_summary(id)?)))
        .await?;
    let mut groups: Vec<Value> = Vec::new();
    for r in callistseo_storage::REPORTS {
        let item = json!({
            "id": r.id, "label": r.label, "columns": r.columns,
            "count": counts.iter().find(|(i, _)| *i == r.id).map_or(0, |c| c.1),
        });
        match groups.iter_mut().find(|g| g["label"] == r.group) {
            Some(g) => g["reports"].as_array_mut().expect("tableau").push(item),
            None => groups.push(json!({"label": r.group, "reports": [item]})),
        }
    }
    let issues: Vec<Value> = summary
        .iter()
        .map(|(rule, severity, count)| {
            let label = rule_info(rule).map_or(rule.as_str(), |i| i.label);
            json!({"id": format!("issue:{rule}"), "rule": rule, "label": label, "severity": severity, "count": count})
        })
        .collect();
    let columns: Vec<Value> = callistseo_storage::COLUMNS
        .iter()
        .map(column_json)
        .collect();
    Ok(Json(
        json!({"groups": groups, "issues": issues, "columns": columns}),
    ))
}

#[derive(Deserialize)]
struct RowsParams {
    report: Option<String>,
    sort: Option<String>,
    dir: Option<String>,
    columns: Option<String>,
    search: Option<String>,
    /// Identifiant d'un segment du projet de l'audit.
    segment: Option<i64>,
    limit: Option<i64>,
    offset: Option<i64>,
}

/// Lignes d'un rapport. Tout identifiant (rapport, colonne, tri, règle) est validé contre le
/// registre : une valeur inconnue est refusée, rien n'est jamais inséré tel quel dans le SQL.
async fn crawl_rows(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Query(p): Query<RowsParams>,
) -> ApiResult<Json<Value>> {
    use callistseo_storage::{column, report, RowQuery, Scope};
    let bad = |m: String| ApiError::new(StatusCode::BAD_REQUEST, m);
    let run = s.require_run(id).await?;
    let segment = match p.segment {
        None => None,
        Some(sid) => {
            let found = s.db(move |st| st.segment(sid)).await?;
            // Un segment d'un autre projet est traité comme inexistant.
            Some(
                found
                    .filter(|(pid, _, _)| *pid == run.project_id)
                    .map(|(_, _, d)| d)
                    .ok_or_else(|| bad("segment inconnu".into()))?,
            )
        }
    };
    let report_id = p.report.as_deref().unwrap_or("all");
    let rule = report_id.strip_prefix("issue:");
    let (def, rule_owned) = match (rule, report(report_id)) {
        (Some(rule), _) if rule_info(rule).is_some() => (None, Some(rule.to_owned())),
        (None, Some(r)) => (Some(r), None),
        _ => return Err(bad(format!("rapport inconnu : {report_id}"))),
    };
    let default_cols: Vec<&str> = match def {
        Some(r) => r.columns.to_vec(),
        None => vec!["url", "status", "title", "issues"],
    };
    let names: Vec<&str> = match p.columns.as_deref() {
        Some(c) if !c.is_empty() => c.split(',').collect(),
        _ => default_cols,
    };
    if names.len() > 30 {
        return Err(bad("trop de colonnes".into()));
    }
    let columns = names
        .iter()
        .map(|n| column(n).ok_or_else(|| bad(format!("colonne inconnue : {n}"))))
        .collect::<Result<Vec<_>, _>>()?;
    let sort = p
        .sort
        .as_deref()
        .map(|n| column(n).ok_or_else(|| bad(format!("tri inconnu : {n}"))))
        .transpose()?;
    let desc = match p.dir.as_deref() {
        None | Some("asc") => false,
        Some("desc") => true,
        Some(d) => return Err(bad(format!("dir : asc ou desc, pas {d}"))),
    };
    let (limit, offset) = (
        p.limit.unwrap_or(100).clamp(1, 500),
        p.offset.unwrap_or(0).max(0),
    );
    let ids: Vec<&'static str> = columns.iter().map(|c| c.id).collect();
    let search = p.search.filter(|q| !q.is_empty());
    let (total, rows) = s
        .db(move |st| {
            let q = RowQuery {
                scope: match (&rule_owned, def) {
                    (Some(r), _) => Scope::Rule(r),
                    (None, Some(d)) => Scope::Report(d),
                    (None, None) => unreachable!("rapport ou règle, validé plus haut"),
                },
                sort,
                desc,
                columns,
                search: search.as_deref(),
                segment: segment.as_ref(),
                limit,
                offset,
            };
            st.report_rows(id, &q)
        })
        .await?;
    Ok(Json(
        json!({"total": total, "columns": ids, "rows": rows, "limit": limit, "offset": offset}),
    ))
}

fn paging(limit: Option<i64>, offset: Option<i64>) -> (i64, i64) {
    (
        limit.unwrap_or(50).clamp(1, 500),
        offset.unwrap_or(0).max(0),
    )
}

fn page_json(p: &CrawledPage) -> Value {
    let f = &p.facts;
    json!({
        "url": p.url.as_str(), "depth": p.depth, "status": p.status,
        "redirect_to": p.redirect_to.as_ref().map(|u| u.as_str()),
        "title": p.meta.title, "description": p.meta.description, "h1": p.meta.h1,
        "canonical": p.meta.canonical.as_ref().map(|u| u.as_str()), "robots": p.meta.robots,
        "in_sitemap": p.in_sitemap, "is_html": p.is_html, "error": p.error,
        "facts": {
            "ttfb_ms": f.ttfb_ms, "total_ms": f.total_ms, "size": f.size,
            "content_type": f.content_type, "headers": f.headers,
            "content_hash": f.content_hash, "word_count": f.word_count, "lang": f.lang,
            "viewport": f.viewport, "h2": f.h2, "hreflang": f.hreflang,
            "images": f.images, "images_total": f.images_total,
            "json_ld": f.json_ld, "nofollow_links": f.nofollow_links,
            "extracted": f.extracted.iter().cloned().collect::<std::collections::BTreeMap<_, _>>(),
        },
    })
}

fn issue_json(i: &Issue) -> Value {
    json!({
        "severity": format!("{:?}", i.severity), "rule": i.rule,
        "url": i.url.as_str(), "message": i.message,
    })
}

#[derive(Deserialize)]
struct PagesParams {
    search: Option<String>,
    status: Option<u16>,
    limit: Option<i64>,
    offset: Option<i64>,
}

async fn crawl_pages(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Query(p): Query<PagesParams>,
) -> ApiResult<Json<Value>> {
    s.require_run(id).await?;
    let (limit, offset) = paging(p.limit, p.offset);
    let query = PageQuery {
        search: p.search.filter(|q| !q.is_empty()),
        status: p.status,
        limit,
        offset,
    };
    let (total, pages) = s.db(move |st| st.pages_page(id, &query)).await?;
    Ok(Json(json!({
        "total": total, "limit": limit, "offset": offset,
        "items": pages.iter().map(page_json).collect::<Vec<_>>(),
    })))
}

#[derive(Deserialize)]
struct IssuesParams {
    /// `Error`, `Warning` ou `Info`.
    severity: Option<String>,
    rule: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

async fn crawl_issues(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Query(p): Query<IssuesParams>,
) -> ApiResult<Json<Value>> {
    s.require_run(id).await?;
    let (limit, offset) = paging(p.limit, p.offset);
    let query = IssueQuery {
        severity: p.severity,
        rule: p.rule,
        limit,
        offset,
    };
    let ((total, issues), summary) = s
        .db(move |st| Ok((st.issues_page(id, &query)?, st.issue_summary(id)?)))
        .await?;
    Ok(Json(json!({
        "total": total, "limit": limit, "offset": offset,
        "items": issues.iter().map(issue_json).collect::<Vec<_>>(),
        "summary": summary.iter()
            .map(|(rule, severity, count)| json!({ "rule": rule, "severity": severity, "count": count }))
            .collect::<Vec<_>>(),
    })))
}

const GRAPH_MAX_NODES: i64 = 5_000;
const GRAPH_MAX_EDGES: i64 = 50_000;

async fn crawl_graph(State(s): State<AppState>, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    s.require_run(id).await?;
    let ((total, nodes), edges) = s
        .db(move |st| {
            Ok((
                st.graph_nodes(id, GRAPH_MAX_NODES)?,
                st.edges(id, GRAPH_MAX_EDGES + 1)?,
            ))
        })
        .await?;
    let edges_truncated = edges.len() as i64 > GRAPH_MAX_EDGES;
    Ok(Json(json!({
        "nodes": nodes.iter()
            .map(|(url, depth, status)| json!({ "url": url, "depth": depth, "status": status }))
            .collect::<Vec<_>>(),
        "edges": edges.iter().take(GRAPH_MAX_EDGES as usize)
            .map(|(source, target)| json!({ "source": source, "target": target }))
            .collect::<Vec<_>>(),
        "truncated": total > GRAPH_MAX_NODES || edges_truncated,
    })))
}

#[derive(Deserialize)]
struct ExportParams {
    /// `json` (défaut) ou `csv`.
    format: Option<String>,
    /// Pour le CSV : `pages` (défaut) ou `issues`.
    kind: Option<String>,
}

#[derive(Deserialize)]
struct SitemapParams {
    part: Option<usize>,
}

/// Sitemap XML des pages indexables de l'audit. Au-delà de 50 000 URL il y a plusieurs fichiers :
/// `?part=N` (l'index suppose l'URL publique du site, que le serveur ne connaît pas).
async fn sitemap_crawl(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Query(p): Query<SitemapParams>,
) -> ApiResult<Response> {
    s.require_run(id).await?;
    let (pages, _) = s.db(move |st| st.load_run(id)).await?;
    let parts = callistseo_export::sitemap_parts(&pages);
    let n = p.part.unwrap_or(1);
    if parts.len() > 1 && p.part.is_none() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            format!(
                "{} fichiers sitemap (50 000 URL max chacun) : ajoutez ?part=1 à ?part={}",
                parts.len(),
                parts.len()
            ),
        ));
    }
    let body = n
        .checked_sub(1)
        .and_then(|i| parts.get(i))
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "fichier sitemap inexistant"))?;
    Ok((
        [
            (
                header::CONTENT_TYPE,
                "application/xml; charset=utf-8".to_owned(),
            ),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"sitemap-{id}-{n}.xml\""),
            ),
        ],
        body.clone(),
    )
        .into_response())
}

async fn export_crawl(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Query(p): Query<ExportParams>,
) -> ApiResult<Response> {
    let run = s.require_run(id).await?;
    let (pages, issues) = s.db(move |st| st.load_run(id)).await?;
    let (content_type, name, body) = match (p.format.as_deref(), p.kind.as_deref()) {
        (Some("html"), _) => (
            "text/html; charset=utf-8",
            format!("crawl-{id}.html"),
            callistseo_export::report_html(
                &callistseo_export::ReportMeta {
                    title: &run.project,
                    start_url: &run.start_url,
                    generated_at: &callistseo_export::iso_utc(run.started_at),
                },
                &pages,
                &issues,
            ),
        ),
        (None | Some("json"), _) => (
            "application/json",
            format!("crawl-{id}.json"),
            callistseo_export::report_json(&pages, &issues),
        ),
        (Some("csv"), None | Some("pages")) => (
            "text/csv; charset=utf-8",
            format!("crawl-{id}-pages.csv"),
            callistseo_export::pages_csv(&pages),
        ),
        (Some("csv"), Some("issues")) => (
            "text/csv; charset=utf-8",
            format!("crawl-{id}-issues.csv"),
            callistseo_export::issues_csv(&issues),
        ),
        _ => {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "format: json, csv ou html ; kind (csv): pages ou issues",
            ))
        }
    };
    Ok((
        [
            (header::CONTENT_TYPE, content_type.to_owned()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{name}\""),
            ),
        ],
        body,
    )
        .into_response())
}

#[derive(Deserialize)]
struct UrlParam {
    url: String,
}

const DETAIL_LINKS_MAX: i64 = 1_000;

/// Une URL de l'audit : ses métadonnées, ses problèmes, qui lui lie et vers quoi elle lie.
async fn page_detail(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Query(p): Query<UrlParam>,
) -> ApiResult<Json<Value>> {
    s.require_run(id).await?;
    let url = normalize(p.url.trim(), None).map_or(p.url, |u| u.to_string());
    let found = s
        .db(move |st| {
            let Some(page) = st.page_by_url(id, &url)? else {
                return Ok(None);
            };
            Ok(Some((
                page,
                st.issues_of_url(id, &url)?,
                st.inlinks(id, &url, DETAIL_LINKS_MAX)?,
                st.outlinks(id, &url)?,
            )))
        })
        .await?;
    let Some((page, issues, (in_total, inlinks), outlinks)) = found else {
        return Err(ApiError::new(
            StatusCode::NOT_FOUND,
            "URL absente de cet audit",
        ));
    };
    Ok(Json(json!({
        "page": page_json(&page),
        "issues": issues.iter().map(issue_json).collect::<Vec<_>>(),
        "inlinks": {
            "total": in_total,
            "items": inlinks.iter()
                .map(|i| json!({ "url": i.url, "status": i.status, "title": i.title }))
                .collect::<Vec<_>>(),
        },
        "outlinks": {
            "total": outlinks.len(),
            "items": outlinks.iter().take(DETAIL_LINKS_MAX as usize)
                .map(|(url, crawled, status)| json!({ "url": url, "crawled": crawled, "status": status }))
                .collect::<Vec<_>>(),
        },
    })))
}

/// Maillage interne : profondeur, pages les plus liées, pages qui lient le plus, pages orphelines.
async fn links_overview(State(s): State<AppState>, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    s.require_run(id).await?;
    let stats = s.db(move |st| st.link_stats(id, 50)).await?;
    let pairs = |v: &[(String, i64)]| -> Vec<Value> {
        v.iter()
            .take(20)
            .map(|(url, count)| json!({ "url": url, "count": count }))
            .collect()
    };
    Ok(Json(json!({
        "internal_links": stats.internal_links,
        "depths": stats.depths.iter()
            .map(|(depth, count)| json!({ "depth": depth, "count": count }))
            .collect::<Vec<_>>(),
        "top_linked": pairs(&stats.top_linked),
        "top_linking": pairs(&stats.top_linking),
        "orphans": { "total": stats.orphans_total, "items": stats.orphans },
    })))
}

/// Longueur maximale de chaque liste renvoyée par la comparaison (les compteurs restent exacts).
const DIFF_LIST_MAX: usize = 500;

/// Compare deux audits terminés d'un même projet : `before` est la référence.
async fn diff_crawls(
    State(s): State<AppState>,
    Path((before_id, after_id)): Path<(i64, i64)>,
) -> ApiResult<Json<Value>> {
    let before = s.require_run(before_id).await?;
    let after = s.require_run(after_id).await?;
    if before.project_id != after.project_id {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "on ne compare que deux audits d'un même projet",
        ));
    }
    if before.status == "running" || after.status == "running" {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "un des audits est encore en cours",
        ));
    }
    let (b, a) = s
        .db(move |st| Ok((st.load_run(before_id)?, st.load_run(after_id)?)))
        .await?;
    let d = callistseo_audit_diff::diff((&b.0, &b.1), (&a.0, &a.1));

    let new_errors = d
        .new_issues
        .iter()
        .filter(|i| i.severity == callistseo_core::Severity::Error)
        .count();
    let truncated = [
        d.added_pages.len(),
        d.removed_pages.len(),
        d.changed_pages.len(),
        d.new_issues.len(),
        d.resolved_issues.len(),
    ]
    .iter()
    .any(|n| *n > DIFF_LIST_MAX);
    fn urls(v: &[Url]) -> Vec<&str> {
        v.iter().take(DIFF_LIST_MAX).map(Url::as_str).collect()
    }
    fn issues(v: &[Issue]) -> Vec<Value> {
        v.iter().take(DIFF_LIST_MAX).map(issue_json).collect()
    }
    let changed: Vec<Value> = d
        .changed_pages
        .iter()
        .take(DIFF_LIST_MAX)
        .map(|c| {
            json!({
                "url": c.url.as_str(),
                "changes": c.changes.iter()
                    .map(|ch| json!({ "field": ch.field, "before": ch.before, "after": ch.after }))
                    .collect::<Vec<_>>(),
            })
        })
        .collect();

    Ok(Json(json!({
        "before": run_json(&before, None),
        "after": run_json(&after, None),
        "counts": {
            "added_pages": d.added_pages.len(), "removed_pages": d.removed_pages.len(),
            "changed_pages": d.changed_pages.len(), "new_issues": d.new_issues.len(),
            "resolved_issues": d.resolved_issues.len(), "new_errors": new_errors,
        },
        "added_pages": urls(&d.added_pages),
        "removed_pages": urls(&d.removed_pages),
        "changed_pages": changed,
        "new_issues": issues(&d.new_issues),
        "resolved_issues": issues(&d.resolved_issues),
        "truncated": truncated,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Method;
    use callistseo_core::{PageMeta, Severity};
    use tower::ServiceExt;
    use url::Url;

    const TOKEN: &str = "0123456789abcdef-token";

    fn app_with(cfg: ApiConfig) -> (Router, AppState) {
        let state = AppState::new(Store::open_in_memory().unwrap(), cfg);
        (router(state.clone()), state)
    }

    fn app() -> (Router, AppState) {
        app_with(ApiConfig::default())
    }

    async fn call(
        app: &Router,
        method: Method,
        uri: &str,
        body: Option<Value>,
        token: Option<&str>,
    ) -> (StatusCode, Vec<u8>) {
        let mut req = Request::builder().method(method).uri(uri);
        if let Some(t) = token {
            req = req.header(header::AUTHORIZATION, format!("Bearer {t}"));
        }
        let req = match body {
            Some(b) => req
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(b.to_string())),
            None => req.body(Body::empty()),
        }
        .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        let status = resp.status();
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        (status, bytes.to_vec())
    }

    async fn json_call(
        app: &Router,
        method: Method,
        uri: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let (status, bytes) = call(app, method, uri, body, None).await;
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    fn u(p: &str) -> Url {
        Url::parse(&format!("http://a.test{p}")).unwrap()
    }

    /// Audit terminé, enregistré directement en base (sans réseau).
    async fn seed(state: &AppState) -> i64 {
        let page = |path: &str, status: u16, title: &str, links: Vec<Url>| CrawledPage {
            facts: Default::default(),
            in_sitemap: false,
            is_html: true,
            meta: PageMeta {
                title: Some(title.into()),
                ..PageMeta::default()
            },
            url: u(path),
            depth: usize::from(path != "/"),
            status: Some(status),
            redirect_to: None,
            links: links.into_iter().map(std::sync::Arc::new).collect(),
            error: None,
        };
        let pages = vec![
            page("/", 200, "Accueil", vec![u("/a"), u("/gone")]),
            page("/a", 200, "Page A", vec![u("/")]),
            page("/gone", 404, "Perdu", vec![]),
        ];
        let issues = vec![
            Issue {
                rule: "http-error",
                severity: Severity::Error,
                url: u("/gone"),
                message: "statut HTTP 404".into(),
            },
            Issue {
                rule: "description-missing",
                severity: Severity::Warning,
                url: u("/a"),
                message: "meta description absente".into(),
            },
        ];
        state
            .db(move |st| st.save_run("demo", &u("/"), &CrawlConfig::default(), &pages, &issues))
            .await
            .unwrap_or_else(|_| panic!("seed"))
    }

    #[tokio::test]
    async fn token_is_required_when_configured() {
        let (app, _) = app_with(ApiConfig {
            token: Some(TOKEN.into()),
            ..ApiConfig::default()
        });
        let get = |t| call(&app, Method::GET, "/api/projects", None, t);
        assert_eq!(get(None).await.0, StatusCode::UNAUTHORIZED);
        assert_eq!(get(Some("faux")).await.0, StatusCode::UNAUTHORIZED);
        assert_eq!(
            get(Some("0123456789abcdef-tokeX")).await.0,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(get(Some(TOKEN)).await.0, StatusCode::OK);
    }

    #[test]
    fn delay_is_raised_to_the_server_minimum_and_capped() {
        let ms = |req, min| clamp_delay(req, min).as_millis();
        assert_eq!(ms(None, 0), 0);
        assert_eq!(ms(Some(250), 0), 250);
        assert_eq!(ms(Some(10), 100), 100); // le serveur impose un minimum
        assert_eq!(ms(None, 100), 100);
        assert_eq!(ms(Some(u64::MAX), 0), 30_000); // plafonné
    }

    #[tokio::test]
    async fn negative_or_malformed_delay_is_rejected() {
        let (app, state) = app();
        seed(&state).await;
        let (status, _) = json_call(
            &app,
            Method::POST,
            "/api/projects/1/crawls",
            Some(json!({"delay_ms": -5})),
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        let (status, _) = json_call(
            &app,
            Method::POST,
            "/api/projects/1/crawls",
            Some(json!({"delay_ms": "vite"})),
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    fn token_comparison() {
        assert!(token_eq(b"abc", b"abc"));
        assert!(!token_eq(b"abc", b"abd") && !token_eq(b"abc", b"ab"));
    }

    #[tokio::test]
    async fn project_validation_and_listing() {
        let (app, _) = app();
        let post = |b| json_call(&app, Method::POST, "/api/projects", Some(b));
        assert_eq!(
            post(json!({"name": "x", "start_url": "ftp://x"})).await.0,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            post(json!({"name": "  ", "start_url": "https://a.test"}))
                .await
                .0,
            StatusCode::BAD_REQUEST
        );
        let (status, created) =
            post(json!({"name": "Site", "start_url": "https://a.test/#x"})).await;
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(created["start_url"], "https://a.test/");
        assert_eq!(
            post(json!({"name": "Site", "start_url": "https://b.test"}))
                .await
                .0,
            StatusCode::CONFLICT
        );

        let (_, list) = json_call(&app, Method::GET, "/api/projects", None).await;
        assert_eq!(list["projects"].as_array().unwrap().len(), 1);
        assert_eq!(
            list["projects"][0]["last_crawl"],
            Value::Null,
            "pas encore d'audit"
        );
        let id = created["id"].as_i64().unwrap();
        let (status, one) =
            json_call(&app, Method::GET, &format!("/api/projects/{id}"), None).await;
        assert_eq!(
            (status, one["crawls"].as_array().unwrap().len()),
            (StatusCode::OK, 0)
        );
        assert_eq!(
            json_call(&app, Method::GET, "/api/projects/999", None)
                .await
                .0,
            StatusCode::NOT_FOUND
        );
    }

    #[tokio::test]
    async fn crawl_runs_in_background_and_ssrf_filter_stays_on() {
        let (app, _) = app();
        let (_, project) = json_call(
            &app,
            Method::POST,
            "/api/projects",
            Some(json!({"name": "local", "start_url": "http://127.0.0.1:9/"})),
        )
        .await;
        let uri = format!("/api/projects/{}/crawls", project["id"]);
        let (status, run) = json_call(&app, Method::POST, &uri, None).await;
        assert_eq!(status, StatusCode::ACCEPTED);
        let run_uri = format!("/api/crawls/{}", run["id"]);

        let mut state = Value::Null;
        for _ in 0..200 {
            state = json_call(&app, Method::GET, &run_uri, None).await.1;
            if state["status"] != "running" {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
        assert_eq!(state["status"], "done", "{state}");
        assert_eq!(state["pages"], 1);

        // L'API ne peut pas viser le réseau local : la page est en erreur « interdite ».
        let (_, pages) = json_call(&app, Method::GET, &format!("{run_uri}/pages"), None).await;
        assert!(pages["items"][0]["error"]
            .as_str()
            .unwrap()
            .contains("interdite"));
        let (_, issues) = json_call(&app, Method::GET, &format!("{run_uri}/issues"), None).await;
        assert_eq!(issues["items"][0]["rule"], "network-error");
    }

    #[tokio::test]
    async fn pages_issues_graph_and_export() {
        let (app, state) = app();
        let run = seed(&state).await;
        let base = format!("/api/crawls/{run}");
        let get = |path: String| {
            let app = app.clone();
            async move { json_call(&app, Method::GET, &path, None).await }
        };

        let (_, p) = get(format!("{base}/pages?limit=2&offset=1")).await;
        assert_eq!(
            (p["total"].as_i64(), p["items"].as_array().unwrap().len()),
            (Some(3), 2)
        );
        assert_eq!(p["items"][0]["url"], "http://a.test/a");
        let (_, p) = get(format!("{base}/pages?search=PAGE")).await;
        assert_eq!(p["total"], 1); // insensible à la casse, sur le title
        let (_, p) = get(format!("{base}/pages?status=404")).await;
        assert_eq!(p["items"][0]["url"], "http://a.test/gone");
        let (_, p) = get(format!("{base}/pages?limit=100000")).await;
        assert_eq!(p["limit"], 500); // plafonné

        let (_, i) = get(format!("{base}/issues")).await;
        assert_eq!(i["total"], 2);
        assert_eq!(i["items"][0]["severity"], "Error");
        assert_eq!(i["summary"][0]["rule"], "http-error");
        let (_, i) = get(format!("{base}/issues?severity=Warning")).await;
        assert_eq!(i["items"][0]["rule"], "description-missing");

        let (_, g) = get(format!("{base}/graph")).await;
        assert_eq!(g["nodes"].as_array().unwrap().len(), 3);
        assert_eq!(g["edges"].as_array().unwrap().len(), 3); // / -> /a, / -> /gone, /a -> /
        assert_eq!(g["truncated"], false);

        // Sitemap : seules les pages indexables, type XML, partie inexistante en 404.
        let sm = |path: String| {
            let app = app.clone();
            async move { call(&app, Method::GET, &path, None, None).await }
        };
        let (status, xml) = sm(format!("{base}/sitemap.xml")).await;
        let xml = String::from_utf8(xml).unwrap();
        assert_eq!(status, StatusCode::OK);
        assert!(
            xml.contains("<loc>http://a.test/a</loc>") && !xml.contains("gone"),
            "{xml}"
        );
        assert_eq!(
            sm(format!("{base}/sitemap.xml?part=2")).await.0,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            sm("/api/crawls/9999/sitemap.xml".into()).await.0,
            StatusCode::NOT_FOUND
        );

        let (status, csv) = call(
            &app,
            Method::GET,
            &format!("{base}/export?format=csv"),
            None,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert!(String::from_utf8(csv)
            .unwrap()
            .starts_with("url,depth,status"));
        let (_, csv) = call(
            &app,
            Method::GET,
            &format!("{base}/export?format=csv&kind=issues"),
            None,
            None,
        )
        .await;
        assert!(String::from_utf8(csv).unwrap().starts_with("severity,rule"));
        let (status, html) = call(
            &app,
            Method::GET,
            &format!("{base}/export?format=html"),
            None,
            None,
        )
        .await;
        let html = String::from_utf8(html).unwrap();
        assert_eq!(status, StatusCode::OK);
        assert!(html.starts_with("<!doctype html>"));
        assert!(html.contains("Audit SEO — demo") && html.contains("Erreur HTTP"));
        let (_, report) = get(format!("{base}/export")).await;
        assert_eq!(report["pages"].as_array().unwrap().len(), 3);
        assert_eq!(
            call(
                &app,
                Method::GET,
                &format!("{base}/export?format=xml"),
                None,
                None
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );

        for sub in ["", "/pages", "/issues", "/graph", "/export"] {
            assert_eq!(
                get(format!("/api/crawls/999{sub}")).await.0,
                StatusCode::NOT_FOUND
            );
        }
    }

    #[tokio::test]
    async fn cancel_marks_running_and_refuses_finished() {
        let (app, state) = app();
        let done = seed(&state).await;
        let cancel = |id: i64| {
            let app = app.clone();
            async move {
                let uri = format!("/api/crawls/{id}/cancel");
                json_call(&app, Method::POST, &uri, None).await
            }
        };
        assert_eq!(cancel(done).await.0, StatusCode::CONFLICT);
        assert_eq!(cancel(999).await.0, StatusCode::NOT_FOUND);

        let running = state
            .db(|st| {
                let pid = st.projects()?[0].id;
                st.create_run(pid, &CrawlConfig::default())
            })
            .await
            .unwrap_or_else(|_| panic!("create_run"));
        let (status, body) = cancel(running).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "cancelled");
        assert_eq!(cancel(running).await.0, StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn pause_resume_and_stop_drive_the_crawl_stats() {
        use std::sync::atomic::Ordering::Relaxed;
        let (app, state) = app();
        seed(&state).await;
        let run = state
            .db(|st| {
                let pid = st.projects()?[0].id;
                st.create_run(pid, &CrawlConfig::default())
            })
            .await
            .unwrap_or_else(|_| panic!("create_run"));
        // Un audit « en cours » : une tâche factice et ses statistiques, comme `spawn_crawl` les enregistre.
        let stats = Arc::new(CrawlStats::default());
        let idle = tokio::spawn(std::future::pending::<()>());
        lock(&state.running).insert(run, (idle.abort_handle(), stats.clone()));

        let post = |action: &'static str| {
            let app = app.clone();
            async move {
                json_call(
                    &app,
                    Method::POST,
                    &format!("/api/crawls/{run}/{action}"),
                    None,
                )
                .await
            }
        };
        let (status, body) = post("pause").await;
        assert_eq!(
            (status, body["live"]["paused"].clone()),
            (StatusCode::OK, true.into())
        );
        assert!(stats.paused.load(Relaxed));
        let (_, body) = post("resume").await;
        assert_eq!(body["live"]["paused"], false);
        let (_, body) = post("stop").await;
        assert_eq!(body["live"]["stopping"], true);
        assert!(stats.stop.load(Relaxed) && !stats.paused.load(Relaxed));

        // Sans audit en cours (inconnu ou terminé) : conflit / introuvable.
        lock(&state.running).remove(&run);
        assert_eq!(post("pause").await.0, StatusCode::CONFLICT);
        idle.abort();
    }

    #[tokio::test]
    async fn project_config_roundtrips_and_is_validated() {
        let (app, state) = app();
        seed(&state).await;
        let pid = state
            .db(|st| Ok(st.projects()?[0].id))
            .await
            .unwrap_or_else(|_| panic!("projet"));
        let path = format!("/api/projects/{pid}/config");
        let put = |body: Value| {
            let app = app.clone();
            let path = path.clone();
            async move { json_call(&app, Method::PUT, &path, Some(body)).await }
        };

        let (status, empty) = json_call(&app, Method::GET, &path, None).await;
        assert_eq!(
            (status, empty["include"].clone()),
            (StatusCode::OK, json!([]))
        );

        let cfg = json!({"include": ["/blog/"], "exclude": ["\\.pdf$"], "strip_params": ["utm_*"],
                         "max_url_length": 200, "robots_txt": "User-agent: *\nAllow: /\n", "max_pages": 50});
        let (status, saved) = put(cfg.clone()).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(saved["strip_params"], json!(["utm_*"]));
        let (_, again) = json_call(&app, Method::GET, &path, None).await;
        assert_eq!(
            (again["max_pages"].clone(), again["include"].clone()),
            (50.into(), json!(["/blog/"]))
        );

        for bad in [
            json!({"include": ["("]}),
            json!({"max_url_length": 3}),
            json!({"robots_txt": "x".repeat(70_000)}),
            json!({"inconnu": 1}),
        ] {
            // 400 (valeur refusée) ou 422 (champ inconnu, rejeté à la lecture du JSON).
            assert!(put(bad.clone()).await.0.is_client_error(), "{bad}");
        }
        // Une mauvaise requête ne remplace pas la configuration enregistrée.
        let (_, kept) = json_call(&app, Method::GET, &path, None).await;
        assert_eq!(kept["max_pages"], 50);
        let missing = json_call(&app, Method::PUT, "/api/projects/9999/config", Some(cfg)).await;
        assert_eq!(missing.0, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn schedule_api_validates_and_scheduler_launches_due_audits_once() {
        let (app, state) = app();
        seed(&state).await;
        let pid = state
            .db(|st| Ok(st.projects()?[0].id))
            .await
            .unwrap_or_else(|_| panic!("projet"));
        let path = format!("/api/projects/{pid}/schedule");
        let call = |m: Method, body: Option<Value>| {
            let app = app.clone();
            let path = path.clone();
            async move { json_call(&app, m, &path, body).await }
        };

        assert_eq!(call(Method::GET, None).await.1, Value::Null);
        for bad in [
            json!({"interval_minutes": 5}),
            json!({"interval_minutes": 99999999}),
            json!({"x": 1}),
        ] {
            assert!(
                call(Method::PUT, Some(bad.clone()))
                    .await
                    .0
                    .is_client_error(),
                "{bad}"
            );
        }
        let (status, sch) = call(Method::PUT, Some(json!({"interval_minutes": 60}))).await;
        assert_eq!(
            (status, sch["enabled"].clone(), sch["last_run_id"].clone()),
            (StatusCode::OK, true.into(), Value::Null)
        );
        assert_eq!(
            json_call(
                &app,
                Method::PUT,
                "/api/projects/9999/schedule",
                Some(json!({"interval_minutes": 60}))
            )
            .await
            .0,
            StatusCode::NOT_FOUND
        );

        // Rien n'est échu : le passage ne lance rien.
        let count =
            |state: AppState| async move { state.db(|st| Ok(st.runs()?.len())).await.unwrap_or(0) };
        let before = count(state.clone()).await;
        run_due_schedules(&state, now_secs()).await;
        assert_eq!(count(state.clone()).await, before);

        // Échéance dépassée : un audit est lancé, l'échéance avance, un second passage ne relance pas.
        let later = now_secs() + 61 * 60;
        run_due_schedules(&state, later).await;
        assert_eq!(count(state.clone()).await, before + 1);
        let (_, after) = call(Method::GET, None).await;
        assert!(after["last_run_id"].is_i64());
        assert!(after["next_run_at"].as_i64().unwrap_or(0) >= later + 3600);
        run_due_schedules(&state, later).await;
        assert_eq!(count(state.clone()).await, before + 1);

        // Désactivée ou supprimée : plus rien.
        assert_eq!(call(Method::DELETE, None).await.0, StatusCode::NO_CONTENT);
        assert_eq!(call(Method::DELETE, None).await.0, StatusCode::NOT_FOUND);
        assert_eq!(call(Method::GET, None).await.1, Value::Null);
    }

    #[tokio::test]
    async fn local_api_accepts_put_and_delete() {
        let api = LocalApi::new(Store::open_in_memory().unwrap(), ApiConfig::default());
        api.call(
            "POST",
            "/api/projects",
            Some(r#"{"name":"L","start_url":"https://a.test/"}"#.into()),
        )
        .await
        .unwrap();
        let put = api
            .call(
                "PUT",
                "/api/projects/1/config",
                Some(r#"{"max_pages":5}"#.into()),
            )
            .await
            .unwrap();
        assert_eq!(put.status, 200);
        assert_eq!(
            api.call(
                "PUT",
                "/api/projects/1/schedule",
                Some(r#"{"interval_minutes":120}"#.into())
            )
            .await
            .unwrap()
            .status,
            200
        );
        assert_eq!(
            api.call("DELETE", "/api/projects/1/schedule", None)
                .await
                .unwrap()
                .status,
            204
        );
        assert!(api.call("PATCH", "/api/projects/1", None).await.is_err());
    }

    #[tokio::test]
    async fn segments_are_validated_stored_counted_and_applied_to_rows() {
        let (app, state) = app();
        let run = seed(&state).await;
        let pid = state
            .db(|st| Ok(st.projects()?[0].id))
            .await
            .unwrap_or_else(|_| panic!("projet"));
        let seg_path = format!("/api/projects/{pid}/segments");
        let post = |body: Value| {
            let app = app.clone();
            let path = seg_path.clone();
            async move { json_call(&app, Method::POST, &path, Some(body)).await }
        };

        for bad in [
            json!({"name": "", "definition": {"status_class": 4}}),
            json!({"name": "vide", "definition": {}}),
            json!({"name": "x", "definition": {"status_class": 9}}),
            json!({"name": "x", "definition": {"inconnu": 1}}),
        ] {
            assert!(post(bad.clone()).await.0.is_client_error(), "{bad}");
        }
        let (status, created) =
            post(json!({"name": "Erreurs 4xx", "definition": {"status_class": 4}})).await;
        assert_eq!(status, StatusCode::CREATED);
        let sid = created["id"].as_i64().unwrap();
        assert_eq!(
            post(json!({"name": "Erreurs 4xx", "definition": {"status_class": 4}}))
                .await
                .0,
            StatusCode::CONFLICT
        );
        assert_eq!(
            json_call(
                &app,
                Method::POST,
                "/api/projects/9999/segments",
                Some(json!({"name": "x", "definition": {"max_depth": 1}}))
            )
            .await
            .0,
            StatusCode::NOT_FOUND
        );

        let (_, listed) = json_call(&app, Method::GET, &seg_path, None).await;
        assert_eq!(listed["segments"][0]["name"], "Erreurs 4xx");
        let (_, counts) = json_call(
            &app,
            Method::GET,
            &format!("/api/crawls/{run}/segments"),
            None,
        )
        .await;
        assert_eq!(counts["segments"][0]["count"], 1);

        let (_, rows) = json_call(
            &app,
            Method::GET,
            &format!("/api/crawls/{run}/rows?report=all&columns=url&segment={sid}"),
            None,
        )
        .await;
        assert_eq!(
            (rows["total"].clone(), rows["rows"][0][0].clone()),
            (1.into(), json!("http://a.test/gone"))
        );
        assert_eq!(
            json_call(
                &app,
                Method::GET,
                &format!("/api/crawls/{run}/rows?segment=9999"),
                None
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );

        assert_eq!(
            json_call(&app, Method::DELETE, &format!("/api/segments/{sid}"), None)
                .await
                .0,
            StatusCode::NO_CONTENT
        );
        assert_eq!(
            json_call(&app, Method::DELETE, &format!("/api/segments/{sid}"), None)
                .await
                .0,
            StatusCode::NOT_FOUND
        );
    }

    #[tokio::test]
    async fn extractors_are_validated_in_the_project_config() {
        let (app, state) = app();
        seed(&state).await;
        let pid = state
            .db(|st| Ok(st.projects()?[0].id))
            .await
            .unwrap_or_else(|_| panic!("projet"));
        let path = format!("/api/projects/{pid}/config");
        let put = |ex: Value| {
            let app = app.clone();
            let path = path.clone();
            async move { json_call(&app, Method::PUT, &path, Some(json!({"extractors": ex}))).await }
        };
        let ok = json!([{"name": "ga", "kind": "regex", "pattern": "UA-(\\d+)"}, {"name": "prix", "kind": "css", "pattern": ".price"}, {"name": "todo", "kind": "contains", "pattern": "TODO"}]);
        assert_eq!(put(ok).await.0, StatusCode::OK);
        for bad in [
            json!([{"name": "a b", "kind": "regex", "pattern": "x"}]),
            json!([{"name": "a", "kind": "xpath", "pattern": "x"}]),
            json!([{"name": "a", "kind": "regex", "pattern": "("}]),
            json!([{"name": "a", "kind": "css", "pattern": "div["}]),
        ] {
            assert!(put(bad.clone()).await.0.is_client_error(), "{bad}");
        }
        let (_, kept) = json_call(&app, Method::GET, &path, None).await;
        assert_eq!(
            kept["extractors"].as_array().unwrap().len(),
            3,
            "la configuration valide est conservée"
        );
    }

    #[tokio::test]
    async fn projects_list_carries_the_latest_audit_of_each_project() {
        let (app, state) = app();
        let first = seed(&state).await;
        let second = state
            .db(|st| {
                let pid = st.projects()?[0].id;
                st.create_run(pid, &CrawlConfig::default())
            })
            .await
            .unwrap_or_else(|_| panic!("create_run"));
        assert!(second > first);
        let (_, list) = json_call(&app, Method::GET, "/api/projects", None).await;
        assert_eq!(list["projects"][0]["last_crawl"]["id"], second);
        assert_eq!(list["projects"][0]["last_crawl"]["status"], "running");
    }

    #[tokio::test]
    async fn deleting_a_project_is_refused_while_an_audit_runs() {
        let (app, state) = app();
        seed(&state).await;
        let pid = state
            .db(|st| Ok(st.projects()?[0].id))
            .await
            .unwrap_or_else(|_| panic!("projet"));
        let running = state
            .db(move |st| st.create_run(pid, &CrawlConfig::default()))
            .await
            .unwrap_or_else(|_| panic!("create_run"));
        let path = format!("/api/projects/{pid}");
        assert_eq!(
            json_call(&app, Method::DELETE, &path, None).await.0,
            StatusCode::CONFLICT
        );
        assert_eq!(
            json_call(&app, Method::GET, &path, None).await.0,
            StatusCode::OK,
            "rien n'a été supprimé"
        );

        // Une fois l'audit annulé, la suppression passe, et tout disparaît.
        assert_eq!(
            json_call(
                &app,
                Method::POST,
                &format!("/api/crawls/{running}/cancel"),
                None
            )
            .await
            .0,
            StatusCode::OK
        );
        assert_eq!(
            json_call(&app, Method::DELETE, &path, None).await.0,
            StatusCode::NO_CONTENT
        );
        assert_eq!(
            json_call(&app, Method::GET, &path, None).await.0,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            json_call(&app, Method::GET, "/api/crawls/1", None).await.0,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            json_call(&app, Method::DELETE, &path, None).await.0,
            StatusCode::NOT_FOUND
        );
        let (_, list) = json_call(&app, Method::GET, "/api/projects", None).await;
        assert_eq!(list["projects"], json!([]));
    }

    #[tokio::test]
    async fn diff_compares_two_runs_of_a_project() {
        let (app, state) = app();
        let first = seed(&state).await;
        // Deuxième audit : /gone corrigé, nouvelle page /new, nouveau problème dessus.
        let second = state
            .db(move |st| {
                let (mut pages, mut issues) = st.load_run(first)?;
                pages[2].status = Some(200);
                let mut new_page = pages[1].clone();
                new_page.url = u("/new");
                new_page.meta.title = None;
                pages.push(new_page);
                issues.retain(|i| i.rule != "http-error");
                issues.push(Issue {
                    rule: "title-missing",
                    severity: Severity::Error,
                    url: u("/new"),
                    message: "balise <title> absente".into(),
                });
                st.save_run("demo", &u("/"), &CrawlConfig::default(), &pages, &issues)
            })
            .await
            .unwrap_or_else(|_| panic!("second run"));

        let (status, d) = json_call(
            &app,
            Method::GET,
            &format!("/api/crawls/{first}/diff/{second}"),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(d["counts"]["added_pages"], 1);
        assert_eq!(d["counts"]["removed_pages"], 0);
        assert_eq!(d["counts"]["new_errors"], 1);
        assert_eq!(d["added_pages"][0], "http://a.test/new");
        assert_eq!(d["changed_pages"][0]["url"], "http://a.test/gone");
        assert_eq!(d["changed_pages"][0]["changes"][0]["field"], "status");
        assert_eq!(d["new_issues"][0]["rule"], "title-missing");
        assert_eq!(d["resolved_issues"][0]["rule"], "http-error");
        assert_eq!(d["truncated"], false);

        let uri = |a: i64, b: i64| format!("/api/crawls/{a}/diff/{b}");
        assert_eq!(
            json_call(&app, Method::GET, &uri(first, 999), None).await.0,
            StatusCode::NOT_FOUND
        );
        // Un autre projet : refusé.
        let other = state
            .db(|st| st.save_run("autre", &u("/"), &CrawlConfig::default(), &[], &[]))
            .await
            .unwrap_or_else(|_| panic!("other"));
        assert_eq!(
            json_call(&app, Method::GET, &uri(first, other), None)
                .await
                .0,
            StatusCode::BAD_REQUEST
        );
        // Un audit encore en cours : refusé.
        let running = state
            .db(move |st| st.create_run(st.projects()?[0].id, &CrawlConfig::default()))
            .await
            .unwrap_or_else(|_| panic!("running"));
        assert_eq!(
            json_call(&app, Method::GET, &uri(first, running), None)
                .await
                .0,
            StatusCode::CONFLICT
        );
    }

    #[tokio::test]
    async fn page_detail_and_links_overview() {
        let (app, state) = app();
        let run = seed(&state).await;
        let get = |path: String| {
            let app = app.clone();
            async move { json_call(&app, Method::GET, &path, None).await }
        };

        let (status, d) = get(format!("/api/crawls/{run}/page?url=http://a.test/a")).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(d["page"]["title"], "Page A");
        assert_eq!(d["inlinks"]["total"], 1);
        assert_eq!(d["inlinks"]["items"][0]["url"], "http://a.test/");
        assert_eq!(d["inlinks"]["items"][0]["status"], 200);
        assert_eq!(d["outlinks"]["items"][0]["url"], "http://a.test/");
        assert_eq!(d["outlinks"]["items"][0]["status"], 200);
        assert_eq!(d["issues"][0]["rule"], "description-missing");
        // L'URL est normalisée avant la recherche (fragment retiré).
        let (status, _) = get(format!("/api/crawls/{run}/page?url=http://a.test/a%23x")).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            get(format!("/api/crawls/{run}/page?url=http://a.test/nope"))
                .await
                .0,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            get(format!("/api/crawls/{run}/page")).await.0,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            get("/api/crawls/999/page?url=http://a.test/".into())
                .await
                .0,
            StatusCode::NOT_FOUND
        );

        let (_, l) = get(format!("/api/crawls/{run}/links")).await;
        assert_eq!(l["internal_links"], 3);
        assert_eq!(l["depths"][0]["count"], 1);
        assert_eq!(l["depths"][1]["count"], 2);
        assert_eq!(l["top_linked"][0]["url"], "http://a.test/");
        assert_eq!(l["orphans"]["total"], 0);
        assert_eq!(
            get("/api/crawls/999/links".into()).await.0,
            StatusCode::NOT_FOUND
        );
    }

    #[tokio::test]
    async fn local_api_calls_the_router_without_network() {
        let api = LocalApi::new(Store::open_in_memory().unwrap(), ApiConfig::default());
        let created = api
            .call(
                "POST",
                "/api/projects",
                Some(r#"{"name":"Local","start_url":"https://a.test/"}"#.into()),
            )
            .await
            .unwrap();
        assert_eq!(created.status, 201);
        assert_eq!(created.content_type.as_deref(), Some("application/json"));

        let list = api.call("GET", "/api/projects", None).await.unwrap();
        assert_eq!(list.status, 200);
        assert!(list.body.contains("\"Local\""), "{}", list.body);
        assert_eq!(
            api.call("GET", "/api/crawls/9", None).await.unwrap().status,
            404
        );

        // Les exports gardent leur type et leur nom de fichier.
        api.call("POST", "/api/projects/1/crawls", None)
            .await
            .unwrap();
        let export = api
            .call("GET", "/api/crawls/1/export?format=csv", None)
            .await
            .unwrap();
        assert_eq!(export.status, 200);
        assert!(export
            .content_disposition
            .unwrap()
            .contains("crawl-1-pages.csv"));
        assert!(export.body.starts_with("url,depth,status"));

        // Seul /api/ en GET/POST/PUT/DELETE est joignable.
        assert!(api.call("GET", "/etc/passwd", None).await.is_err());
        assert!(api.call("GET", "//api/projects", None).await.is_err());
        assert!(api.call("PATCH", "/api/projects", None).await.is_err());
    }

    /// Réponse brute (en-têtes + corps) d'un GET sur `http_app`, avec l'en-tête Accept-Encoding donné.
    async fn raw_get(
        app: &Router,
        uri: &str,
        accept: Option<&str>,
    ) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
        use tower::ServiceExt;
        let mut req = Request::builder().uri(uri);
        if let Some(a) = accept {
            req = req.header(header::ACCEPT_ENCODING, a);
        }
        let resp = app
            .clone()
            .oneshot(req.body(Body::empty()).unwrap())
            .await
            .unwrap();
        let (status, headers) = (resp.status(), resp.headers().clone());
        (
            status,
            headers,
            axum::body::to_bytes(resp.into_body(), usize::MAX)
                .await
                .unwrap()
                .to_vec(),
        )
    }

    #[tokio::test]
    async fn http_responses_are_compressed_on_request_and_cached_sensibly() {
        let root = std::env::temp_dir().join(format!("callistseo-gz-{}", std::process::id()));
        let dir = root.join("dist");
        std::fs::create_dir_all(dir.join("assets")).unwrap();
        std::fs::write(dir.join("index.html"), "<html>INTERFACE</html>").unwrap();
        let js = "const a = 'répétitif répétitif répétitif';\n".repeat(2000);
        std::fs::write(dir.join("assets/app-abc123.js"), &js).unwrap();
        let state = AppState::new(
            Store::open_in_memory().unwrap(),
            ApiConfig {
                static_dir: Some(dir),
                ..ApiConfig::default()
            },
        );
        seed(&state).await;
        let app = http_app(state);
        let enc = |h: &axum::http::HeaderMap| {
            h.get(header::CONTENT_ENCODING)
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned)
        };
        let cache = |h: &axum::http::HeaderMap| {
            h.get(header::CACHE_CONTROL)
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned)
        };

        // Sans Accept-Encoding : rien n'est compressé (clients simples, curl).
        let (_, h, plain) = raw_get(&app, "/assets/app-abc123.js", None).await;
        assert_eq!((enc(&h), plain.len()), (None, js.len()));
        // gzip puis brotli, selon la demande ; le corps est beaucoup plus petit.
        let (_, h, gz) = raw_get(&app, "/assets/app-abc123.js", Some("gzip")).await;
        assert_eq!(enc(&h).as_deref(), Some("gzip"));
        assert!(gz.len() < js.len() / 10, "{} → {}", js.len(), gz.len());
        let (_, h, br) = raw_get(&app, "/assets/app-abc123.js", Some("br, gzip")).await;
        assert_eq!(enc(&h).as_deref(), Some("br"));
        assert!(br.len() < js.len() / 10);
        // Le JSON de l'API aussi.
        let (_, h, _) = raw_get(&app, "/api/crawls/1/rows?limit=200", Some("gzip")).await;
        assert_eq!(enc(&h).as_deref(), Some("gzip"));

        // Cache : fichiers hachés immuables, page d'entrée revalidée, API jamais en cache.
        let (_, h, _) = raw_get(&app, "/assets/app-abc123.js", None).await;
        assert_eq!(
            cache(&h).as_deref(),
            Some("public, max-age=31536000, immutable")
        );
        let (_, h, _) = raw_get(&app, "/", None).await;
        assert_eq!(cache(&h).as_deref(), Some("no-cache"));
        let (_, h, _) = raw_get(&app, "/api/projects", None).await;
        assert_eq!(cache(&h).as_deref(), Some("no-store"));
        // Un fichier absent ne reçoit pas le cache d'un an.
        let (status, h, _) = raw_get(&app, "/assets/inconnu.js", None).await;
        assert_ne!(
            cache(&h).as_deref(),
            Some("public, max-age=31536000, immutable"),
            "{status}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn failed_logins_are_limited_per_client() {
        use tower::ServiceExt;
        let state = AppState::new(
            Store::open_in_memory().unwrap(),
            ApiConfig {
                token: Some(TOKEN.into()),
                trust_proxy: true,
                ..ApiConfig::default()
            },
        );
        let app = http_app(state);
        let get = |ip: &str, token: &str| {
            let req = Request::builder()
                .uri("/api/projects")
                // Le client ne choisit que le début de la liste : seule la dernière entrée compte.
                .header("x-forwarded-for", format!("6.6.6.6, {ip}"))
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap();
            let app = app.clone();
            async move { app.oneshot(req).await.unwrap() }
        };
        for _ in 0..AUTH_FAILS {
            assert_eq!(
                get("1.1.1.1", "faux").await.status(),
                StatusCode::UNAUTHORIZED
            );
        }
        // Bloquée, même avec le bon jeton (pas d'oracle pendant le blocage).
        let resp = get("1.1.1.1", TOKEN).await;
        assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
        assert!(resp.headers().contains_key(header::RETRY_AFTER));
        // Une autre adresse n'est pas touchée ; les en-têtes de sécurité sont posés.
        let resp = get("2.2.2.2", TOKEN).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let csp = resp.headers()[header::CONTENT_SECURITY_POLICY]
            .to_str()
            .unwrap();
        assert!(csp.contains("frame-ancestors 'none'") && csp.contains("script-src 'self'"));
        assert_eq!(resp.headers()[header::X_CONTENT_TYPE_OPTIONS], "nosniff");

        // La fenêtre passée, l'adresse est libérée.
        let lim = AuthLimiter::default();
        let (ip, t0) = (IpAddr::from([1, 1, 1, 1]), Instant::now());
        (0..AUTH_FAILS).for_each(|_| lim.fail(ip, t0));
        assert!(lim.blocked(ip, t0).is_some());
        assert!(lim.blocked(ip, t0 + AUTH_WINDOW).is_none());
    }

    #[tokio::test]
    async fn static_ui_is_public_while_api_needs_token() {
        let root = std::env::temp_dir().join(format!("callistseo-ui-{}", std::process::id()));
        let dir = root.join("dist");
        std::fs::create_dir_all(dir.join("assets")).unwrap();
        std::fs::write(dir.join("index.html"), "<html>INTERFACE</html>").unwrap();
        std::fs::write(dir.join("assets/app.js"), "console.log(1)").unwrap();
        std::fs::write(root.join("secret.txt"), "SECRET").unwrap();

        let (app, _) = app_with(ApiConfig {
            token: Some(TOKEN.into()),
            static_dir: Some(dir),
            ..ApiConfig::default()
        });
        let text = |status_body: (StatusCode, Vec<u8>)| {
            (
                status_body.0,
                String::from_utf8_lossy(&status_body.1).into_owned(),
            )
        };
        let get = |uri: &str, token: Option<&str>| {
            let (app, uri, token) = (app.clone(), uri.to_owned(), token.map(str::to_owned));
            async move { text(call(&app, Method::GET, &uri, None, token.as_deref()).await) }
        };

        // Interface : publique, avec repli vers index.html pour les routes du client.
        for uri in ["/", "/projects/1", "/crawls/3/url"] {
            let (status, body) = get(uri, None).await;
            assert!(
                status == StatusCode::OK && body.contains("INTERFACE"),
                "{uri}"
            );
        }
        let (status, body) = get("/assets/app.js", None).await;
        assert!(status == StatusCode::OK && body.contains("console.log"));

        // API : protégée, sauf la sonde de santé ; une route inconnue est un 404 JSON, pas l'interface.
        assert_eq!(get("/api/health", None).await.0, StatusCode::OK);
        assert_eq!(get("/api/projects", None).await.0, StatusCode::UNAUTHORIZED);
        assert_eq!(get("/api/projects", Some(TOKEN)).await.0, StatusCode::OK);
        let (status, body) = get("/api/inconnue", Some(TOKEN)).await;
        assert!(
            status == StatusCode::NOT_FOUND && body.contains("route inconnue"),
            "{body}"
        );
        assert_eq!(get("/api/inconnue", None).await.0, StatusCode::UNAUTHORIZED);

        // Aucune sortie du dossier servi.
        for uri in [
            "/..%2fsecret.txt",
            "/%2e%2e/secret.txt",
            "/assets/..%2f..%2fsecret.txt",
            "/../secret.txt",
        ] {
            let (_, body) = get(uri, None).await;
            assert!(
                !body.contains("SECRET"),
                "{uri} a servi un fichier hors du dossier"
            );
        }
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[tokio::test]
    async fn serve_refuses_unsafe_setups() {
        let store = || Store::open_in_memory().unwrap();
        let public: SocketAddr = "0.0.0.0:0".parse().unwrap();
        let local: SocketAddr = "127.0.0.1:0".parse().unwrap();
        assert!(serve(public, store(), ApiConfig::default()).await.is_err());
        let weak = ApiConfig {
            token: Some("court".into()),
            ..ApiConfig::default()
        };
        assert!(serve(local, store(), weak).await.is_err());
        // Un identifiant de robot invalide empêche le démarrage (en-tête HTTP inutilisable).
        let bad_agent = ApiConfig {
            user_agent: "a\r\nX-Evil: 1".into(),
            ..ApiConfig::default()
        };
        assert!(serve(local, store(), bad_agent).await.is_err());
    }

    #[tokio::test]
    async fn reports_and_rows_are_validated_against_the_registry() {
        let (app, state) = app();
        let run = seed(&state).await;
        let get = |path: String| {
            let app = app.clone();
            async move { json_call(&app, Method::GET, &path, None).await }
        };
        let (status, r) = get(format!("/api/crawls/{run}/reports")).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(r["groups"][0]["label"], "Vue d'ensemble");
        assert_eq!(r["groups"][0]["reports"][0]["id"], "all");
        assert_eq!(r["groups"][0]["reports"][0]["count"], 3);
        assert_eq!(r["issues"][0]["id"], "issue:http-error");
        assert!(r["columns"].as_array().unwrap().len() > 10);

        let base = format!("/api/crawls/{run}/rows");
        let (_, rows) = get(format!(
            "{base}?report=all&columns=url,status&sort=url&dir=desc&limit=2"
        ))
        .await;
        assert_eq!(
            (
                rows["total"].clone(),
                rows["rows"].as_array().unwrap().len()
            ),
            (3.into(), 2)
        );
        assert_eq!(rows["columns"], json!(["url", "status"]));
        let (_, by_rule) = get(format!("{base}?report=issue:http-error")).await;
        assert_eq!(by_rule["total"], 1);

        for bad in [
            "report=nope",
            "report=issue:nope",
            "columns=url,p.url%3B%20DROP%20TABLE%20pages",
            "sort=title%20DESC;--",
            "dir=sideways",
        ] {
            assert_eq!(
                get(format!("{base}?{bad}")).await.0,
                StatusCode::BAD_REQUEST,
                "{bad}"
            );
        }
        assert_eq!(
            get("/api/crawls/9999/rows".into()).await.0,
            StatusCode::NOT_FOUND
        );
        // Un audit terminé ne se met pas en pause.
        let (status, _) = json_call(
            &app,
            Method::POST,
            &format!("/api/crawls/{run}/pause"),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT);
    }
}
