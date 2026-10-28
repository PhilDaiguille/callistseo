use callistseo_core::CrawlConfig;
use callistseo_crawler::crawl;
use std::collections::HashMap;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use url::Url;

type Route = (&'static str, u16, &'static str, &'static str);

/// Mini-serveur : chemin -> (statut, en-têtes, corps). Tout le reste répond 404.
async fn serve(routes: &'static [Route]) -> Url {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let table: HashMap<&str, Route> = routes.iter().map(|r| (r.0, *r)).collect();
    tokio::spawn(async move {
        loop {
            let (mut s, _) = listener.accept().await.unwrap();
            let table = table.clone();
            tokio::spawn(async move {
                let mut buf = [0u8; 4096];
                let n = s.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]);
                let path = req.split_whitespace().nth(1).unwrap_or("/");
                let (status, headers, body) = table
                    .get(path)
                    .map(|r| (r.1, r.2, r.3))
                    .unwrap_or((404, "", "nope"));
                let body = body.replace("{PORT}", &port.to_string());
                let resp = format!(
                    "HTTP/1.1 {status} X\r\nContent-Type: text/html\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = s.write_all(resp.as_bytes()).await;
            });
        }
    });
    Url::parse(&format!("http://127.0.0.1:{port}/")).unwrap()
}

fn local_cfg() -> CrawlConfig {
    CrawlConfig {
        allow_private: true,
        ..CrawlConfig::default()
    }
}

static SITE: &[Route] = &[
    (
        "/",
        200,
        "",
        r#"<a href="/a">a</a><a href="/a#frag">a2</a><a href="/broken">b</a><a href="http://other.test/x">ext</a><a href="/old">old</a>"#,
    ),
    (
        "/a",
        200,
        "",
        r#"<a href="/">home</a><a href="/deep">d</a>"#,
    ),
    ("/deep", 200, "", r#"<a href="/deeper">d</a>"#),
    ("/deeper", 200, "", "end"),
    ("/old", 301, "Location: /loop\r\n", ""),
    ("/loop", 302, "Location: /old\r\n", ""),
];

#[tokio::test]
async fn discovers_pages_dedups_and_handles_errors() {
    let start = serve(SITE).await;
    let mut pages = crawl(start, local_cfg()).await.unwrap();
    pages.sort_by(|a, b| a.url.cmp(&b.url));
    let by_path = |p: &str| {
        pages
            .iter()
            .find(|x| x.url.path() == p)
            .unwrap_or_else(|| panic!("{p} manquant"))
    };

    assert_eq!(
        pages.len(),
        7,
        "doublons, fragments ou lien externe mal gérés"
    );
    assert_eq!(by_path("/broken").status, Some(404));
    assert_eq!(by_path("/old").status, Some(301));
    assert_eq!(
        by_path("/old").redirect_to.as_ref().unwrap().path(),
        "/loop"
    ); // boucle : terminée, sans panic
    assert_eq!(by_path("/deeper").depth, 3);
}

#[tokio::test]
async fn respects_limits() {
    let start = serve(SITE).await;
    let depth1 = crawl(
        start.clone(),
        CrawlConfig {
            max_depth: 1,
            ..local_cfg()
        },
    )
    .await
    .unwrap();
    assert!(depth1.iter().all(|p| p.depth <= 1) && !depth1.iter().any(|p| p.url.path() == "/deep"));
    let capped = crawl(
        start,
        CrawlConfig {
            max_pages: 2,
            ..local_cfg()
        },
    )
    .await
    .unwrap();
    assert_eq!(capped.len(), 2);
}

#[tokio::test]
async fn refuses_private_destinations_by_default() {
    let start = serve(SITE).await; // 127.0.0.1
    let pages = crawl(start, CrawlConfig::default()).await.unwrap();
    assert_eq!(pages.len(), 1);
    assert!(pages[0].error.as_ref().unwrap().contains("interdite"));
    assert_eq!(pages[0].status, None);
}

static ROBOTS_SITE: &[Route] = &[
    (
        "/",
        200,
        "",
        r#"<a href="/ok">ok</a><a href="/secret/x">s</a>"#,
    ),
    ("/ok", 200, "", "ok"),
    ("/robots.txt", 200, "", "User-agent: *\nDisallow: /secret\n"),
];

#[tokio::test]
async fn honors_robots_txt_unless_ignored() {
    let start = serve(ROBOTS_SITE).await;
    let pages = crawl(start.clone(), local_cfg()).await.unwrap();
    let secret = pages.iter().find(|p| p.url.path() == "/secret/x").unwrap();
    assert!(secret.status.is_none() && secret.error.as_ref().unwrap().contains("robots"));
    assert_eq!(
        pages.iter().find(|p| p.url.path() == "/ok").unwrap().status,
        Some(200)
    );

    let cfg = CrawlConfig {
        respect_robots: false,
        ..local_cfg()
    };
    let pages = crawl(start, cfg).await.unwrap();
    assert_eq!(
        pages
            .iter()
            .find(|p| p.url.path() == "/secret/x")
            .unwrap()
            .status,
        Some(404)
    );
}

static SITEMAP_SITE: &[Route] = &[
    ("/", 200, "", r#"<a href="/linked">l</a>"#),
    ("/linked", 200, "", "ok"),
    ("/hidden", 200, "", "ok"),
    ("/robots.txt", 200, "", "Sitemap: http://127.0.0.1:{PORT}/sm.xml\n"),
    (
        "/sm.xml",
        200,
        "",
        "<urlset><url><loc>http://127.0.0.1:{PORT}/linked</loc></url><url><loc>http://127.0.0.1:{PORT}/hidden</loc></url></urlset>",
    ),
];

#[tokio::test]
async fn reads_sitemaps_and_crawls_sitemap_only_urls() {
    let pages = crawl(serve(SITEMAP_SITE).await, local_cfg()).await.unwrap();
    let get = |p: &str| {
        pages
            .iter()
            .find(|x| x.url.path() == p)
            .unwrap_or_else(|| panic!("{p} manquant"))
    };
    assert!(get("/linked").in_sitemap && get("/hidden").in_sitemap);
    assert!(!get("/").in_sitemap);
    assert_eq!(get("/hidden").status, Some(200)); // jamais lié : trouvé via le sitemap
}

static CDN_SITE: &[Route] = &[
    (
        "/",
        200,
        "",
        r#"<a href="/cdn-cgi/l/email-protection#abcdef">mail</a><a href="/cdn-cgi/l/email-protection">m2</a><a href="/real">r</a>"#,
    ),
    ("/real", 200, "", "ok"),
];

#[tokio::test]
async fn skips_cdn_infrastructure_urls() {
    let pages = crawl(serve(CDN_SITE).await, local_cfg()).await.unwrap();
    let mut paths: Vec<_> = pages.iter().map(|p| p.url.path().to_owned()).collect();
    paths.sort();
    assert_eq!(
        paths,
        ["/", "/real"],
        "/cdn-cgi/ ne doit pas être exploré (faux 404 Cloudflare)"
    );
}

// ---- politesse : délai, Crawl-delay, 429/503 -------------------------------------------------

static CHAIN_SITE: &[Route] = &[
    (
        "/",
        200,
        "",
        r#"<a href="/a">a</a><a href="/b">b</a><a href="/c">c</a><a href="/d">d</a>"#,
    ),
    ("/a", 200, "", "ok"),
    ("/b", 200, "", "ok"),
    ("/c", 200, "", "ok"),
    ("/d", 200, "", "ok"),
];

#[tokio::test]
async fn delay_spaces_out_requests_even_with_high_concurrency() {
    let cfg = CrawlConfig {
        delay: std::time::Duration::from_millis(50),
        concurrency: 8, // la concurrence n'autorise pas à aller plus vite que le délai
        ..local_cfg()
    };
    let start = serve(CHAIN_SITE).await;
    let t = std::time::Instant::now();
    let pages = crawl(start, cfg).await.unwrap();
    assert_eq!(pages.len(), 5);
    // 5 requêtes espacées de 50 ms : au moins 4 intervalles (borne basse, donc non fragile).
    assert!(
        t.elapsed() >= std::time::Duration::from_millis(200),
        "durée {:?}",
        t.elapsed()
    );
}

static CRAWL_DELAY_SITE: &[Route] = &[
    (
        "/",
        200,
        "",
        r#"<a href="/a">a</a><a href="/b">b</a><a href="/c">c</a>"#,
    ),
    ("/a", 200, "", "ok"),
    ("/b", 200, "", "ok"),
    ("/c", 200, "", "ok"),
    ("/robots.txt", 200, "", "User-agent: *\nCrawl-delay: 0.2\n"),
];

#[tokio::test]
async fn robots_crawl_delay_is_applied_unless_robots_are_ignored() {
    let start = serve(CRAWL_DELAY_SITE).await;
    let t = std::time::Instant::now();
    let pages = crawl(start.clone(), local_cfg()).await.unwrap();
    assert_eq!(pages.len(), 4);
    assert!(
        t.elapsed() >= std::time::Duration::from_millis(600),
        "Crawl-delay ignoré : {:?}",
        t.elapsed()
    );

    let t = std::time::Instant::now();
    let cfg = CrawlConfig {
        respect_robots: false,
        ..local_cfg()
    };
    assert_eq!(crawl(start, cfg).await.unwrap().len(), 4);
    assert!(
        t.elapsed() < std::time::Duration::from_millis(400),
        "robots ignoré mais ralenti : {:?}",
        t.elapsed()
    );
}

/// Serveur à état : le gestionnaire reçoit le chemin et le numéro de la requête (1, 2, …) sur ce chemin.
async fn serve_with(
    handler: impl Fn(&str, usize) -> (u16, &'static str, &'static str) + Send + Sync + 'static,
) -> Url {
    serve_with_delay(std::time::Duration::ZERO, handler).await
}

/// Comme `serve_with`, avec une latence fixe par réponse.
async fn serve_with_delay(
    delay: std::time::Duration,
    handler: impl Fn(&str, usize) -> (u16, &'static str, &'static str) + Send + Sync + 'static,
) -> Url {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let handler = std::sync::Arc::new(handler);
    let hits = std::sync::Arc::new(std::sync::Mutex::new(HashMap::<String, usize>::new()));
    tokio::spawn(async move {
        loop {
            let (mut s, _) = listener.accept().await.unwrap();
            let (handler, hits) = (handler.clone(), hits.clone());
            tokio::spawn(async move {
                let mut buf = [0u8; 4096];
                let n = s.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]);
                let path = req.split_whitespace().nth(1).unwrap_or("/").to_owned();
                let nth = {
                    let mut h = hits.lock().unwrap();
                    let c = h.entry(path.clone()).or_default();
                    *c += 1;
                    *c
                };
                let (status, headers, body) = handler(&path, nth);
                tokio::time::sleep(delay).await;
                let resp = format!(
                    "HTTP/1.1 {status} X\r\nContent-Type: text/html\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = s.write_all(resp.as_bytes()).await;
            });
        }
    });
    Url::parse(&format!("http://127.0.0.1:{port}/")).unwrap()
}

#[tokio::test]
async fn rate_limited_responses_pause_the_crawl_and_are_retried_once() {
    let down_hits = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counter = down_hits.clone();
    let start = serve_with(move |path, nth| match path {
        "/" => (200, "", r#"<a href="/flaky">f</a><a href="/down">d</a>"#),
        // 429 la première fois, puis tout va bien.
        "/flaky" if nth == 1 => (429, "Retry-After: 1\r\n", "slow down"),
        "/flaky" => (200, "", "ok"),
        // 503 sans Retry-After, en permanence : pause par défaut (1 s), une seule reprise.
        "/down" => {
            counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            (503, "", "down")
        }
        _ => (404, "", "nope"),
    })
    .await;

    let t = std::time::Instant::now();
    let pages = crawl(start, local_cfg()).await.unwrap();
    let status = |p: &str| pages.iter().find(|x| x.url.path() == p).unwrap().status;
    assert_eq!(status("/flaky"), Some(200), "retentée après la pause");
    assert_eq!(
        status("/down"),
        Some(503),
        "le statut est conservé après la reprise"
    );
    assert_eq!(
        down_hits.load(std::sync::atomic::Ordering::SeqCst),
        2,
        "une requête + une reprise"
    );
    assert!(
        t.elapsed() >= std::time::Duration::from_secs(1),
        "pas de pause : {:?}",
        t.elapsed()
    );
    assert_eq!(pages.len(), 3, "chaque URL n'apparaît qu'une fois");
}

/// Non-régression mémoire (trouvé avec dhat) : `into_iter().map().collect()` réutilisait le tampon
/// de `Vec<Url>` et chaque page gardait ≈ 45 Ko de capacité inutilisée pour ses liens.
#[tokio::test]
async fn page_links_do_not_retain_oversized_buffers() {
    let home: &'static str = Box::leak(
        (0..300)
            .map(|i| format!(r#"<a href="/p{i}">p{i}</a>"#))
            .collect::<String>()
            .into_boxed_str(),
    );
    let start = serve_with(move |path, _| {
        if path == "/" {
            (200, "", home)
        } else {
            (404, "", "x")
        }
    })
    .await;
    let pages = crawl(
        start,
        CrawlConfig {
            max_pages: 400,
            ..local_cfg()
        },
    )
    .await
    .unwrap();
    let home = pages.iter().find(|p| p.url.path() == "/").unwrap();
    assert_eq!(home.links.len(), 300);
    assert!(
        home.links.capacity() <= home.links.len() * 2,
        "capacité {} pour {} liens : tampon surdimensionné conservé",
        home.links.capacity(),
        home.links.len()
    );
}

// ---- concurrence adaptative ------------------------------------------------------------------

/// Page d'accueil avec `n` liens `/p0`…`/p{n-1}` ; tout le reste répond via `other`.
fn home_with_links(n: usize) -> &'static str {
    Box::leak(
        (0..n)
            .map(|i| format!(r#"<a href="/p{i}">p{i}</a>"#))
            .collect::<String>()
            .into_boxed_str(),
    )
}

fn adaptive_cfg(start: usize, max: usize) -> CrawlConfig {
    CrawlConfig {
        concurrency: start,
        max_concurrency: max,
        max_pages: 1000,
        ..local_cfg()
    }
}

#[tokio::test]
async fn adaptive_concurrency_climbs_on_a_steady_server() {
    let home = home_with_links(120);
    let start = serve_with_delay(std::time::Duration::from_millis(30), move |p, _| {
        if p == "/" {
            (200, "", home)
        } else {
            (200, "", "<html><title>t</title></html>")
        }
    })
    .await;
    let stats = std::sync::Arc::new(callistseo_crawler::CrawlStats::default());
    let pages = callistseo_crawler::crawl_observed(start, adaptive_cfg(2, 8), stats.clone())
        .await
        .unwrap();
    assert_eq!(pages.len(), 121);
    let load = |a: &std::sync::atomic::AtomicUsize| a.load(std::sync::atomic::Ordering::Relaxed);
    assert_eq!(
        load(&stats.limit),
        8,
        "la concurrence doit monter jusqu'au plafond"
    );
    assert_eq!((load(&stats.completed), load(&stats.in_flight)), (121, 0));
    assert!(
        stats.urls_per_second() > 0.0
            && stats.avg_latency() >= std::time::Duration::from_millis(25)
    );
}

#[tokio::test]
async fn adaptive_concurrency_does_not_climb_while_the_server_errors() {
    let home = home_with_links(120);
    // Un quart des pages répond 500 : chaque fenêtre de 10 requêtes contient un échec.
    let start = serve_with(move |p, _| {
        let n: usize = p.trim_start_matches("/p").parse().unwrap_or(1);
        match (p, n % 4) {
            ("/", _) => (200, "", home),
            (_, 0) => (500, "", "boom"),
            _ => (200, "", "<html></html>"),
        }
    })
    .await;
    let stats = std::sync::Arc::new(callistseo_crawler::CrawlStats::default());
    callistseo_crawler::crawl_observed(start, adaptive_cfg(4, 16), stats.clone())
        .await
        .unwrap();
    let load = |a: &std::sync::atomic::AtomicUsize| a.load(std::sync::atomic::Ordering::Relaxed);
    assert_eq!(
        load(&stats.limit),
        4,
        "pas de montée tant que le serveur renvoie des 5xx"
    );
    assert!(
        load(&stats.failures) >= 20,
        "échecs comptés : {}",
        load(&stats.failures)
    );
}

#[tokio::test]
async fn a_delay_fixes_the_concurrency() {
    let home = home_with_links(30);
    let start = serve_with(move |p, _| {
        if p == "/" {
            (200, "", home)
        } else {
            (200, "", "<html></html>")
        }
    })
    .await;
    let cfg = CrawlConfig {
        delay: std::time::Duration::from_millis(5),
        ..adaptive_cfg(2, 8)
    };
    let stats = std::sync::Arc::new(callistseo_crawler::CrawlStats::default());
    callistseo_crawler::crawl_observed(start, cfg, stats.clone())
        .await
        .unwrap();
    assert_eq!(stats.limit.load(std::sync::atomic::Ordering::Relaxed), 2);
}

#[tokio::test]
async fn response_and_content_facts_are_recorded() {
    let start = serve_with_delay(std::time::Duration::from_millis(40), |p, _| match p {
        "/" => (
            200,
            "X-Robots-Tag: noindex\r\nStrict-Transport-Security: max-age=31536000\r\nX-Powered-By: ignoré\r\n",
            "<html lang=\"fr\"><title>t</title><body><p>un deux trois</p><a href=\"/img.png\">i</a></body></html>",
        ),
        _ => (404, "", "nope"),
    })
    .await;
    let pages = callistseo_crawler::crawl(start, local_cfg()).await.unwrap();
    let home = pages.iter().find(|p| p.url.path() == "/").unwrap();
    let f = &home.facts;
    assert!(f.ttfb_ms.unwrap() >= 35 && f.total_ms >= f.ttfb_ms, "{f:?}");
    assert_eq!(f.content_type.as_deref(), Some("text/html"));
    assert_eq!(f.header("x-robots-tag"), Some("noindex"));
    assert_eq!(
        f.header("strict-transport-security"),
        Some("max-age=31536000")
    );
    assert_eq!(
        f.header("x-powered-by"),
        None,
        "seuls les en-têtes utiles sont conservés"
    );
    assert_eq!((f.lang.as_deref(), f.word_count), (Some("fr"), Some(4)));
    assert_eq!(f.size, Some(home.facts.size.unwrap()));
    // Une réponse non 2xx n'est pas lue, mais sa taille annoncée est connue.
    let img = pages.iter().find(|p| p.url.path() == "/img.png").unwrap();
    assert_eq!(
        (img.facts.size, img.facts.content_hash.as_deref()),
        (Some(4), None)
    );
}

#[tokio::test]
async fn pause_holds_new_requests_until_resumed() {
    use std::sync::atomic::Ordering::Relaxed;
    let home = home_with_links(20);
    let start = serve_with(move |p, _| {
        if p == "/" {
            (200, "", home)
        } else {
            (200, "", "<html></html>")
        }
    })
    .await;
    let stats = std::sync::Arc::new(callistseo_crawler::CrawlStats::default());
    stats.paused.store(true, Relaxed);
    let task = tokio::spawn(callistseo_crawler::crawl_observed(
        start,
        local_cfg(),
        stats.clone(),
    ));
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    assert_eq!(
        stats.completed.load(Relaxed),
        0,
        "rien ne part tant que c'est en pause"
    );
    assert!(!task.is_finished());
    stats.paused.store(false, Relaxed);
    assert_eq!(task.await.unwrap().unwrap().len(), 21);
}

#[tokio::test]
async fn stop_returns_the_pages_already_crawled() {
    use std::sync::atomic::Ordering::Relaxed;
    let home = home_with_links(300);
    let start = serve_with_delay(std::time::Duration::from_millis(20), move |p, _| {
        if p == "/" {
            (200, "", home)
        } else {
            (200, "", "<html></html>")
        }
    })
    .await;
    let stats = std::sync::Arc::new(callistseo_crawler::CrawlStats::default());
    let cfg = CrawlConfig {
        concurrency: 2,
        max_concurrency: 2,
        ..local_cfg()
    };
    let task = tokio::spawn(callistseo_crawler::crawl_observed(
        start,
        cfg,
        stats.clone(),
    ));
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    stats.stop.store(true, Relaxed);
    let pages = task.await.unwrap().unwrap();
    assert!(
        pages.len() > 2 && pages.len() < 301,
        "{} pages",
        pages.len()
    );
}

fn scoped(scope: callistseo_core::UrlScope) -> CrawlConfig {
    CrawlConfig {
        scope,
        ..local_cfg()
    }
}

const SCOPE_HOME: &str = r#"<a href="/blog/a?utm_source=x">a</a><a href="/blog/a?utm_source=y&sid=1">a2</a>
    <a href="/blog/doc.pdf">pdf</a><a href="/shop/x">shop</a><a href="/blog/b">b</a>"#;

#[tokio::test]
async fn scope_filters_and_normalizes_discovered_urls() {
    let start = serve_with(|p, _| {
        if p == "/" {
            (200, "", SCOPE_HOME)
        } else {
            (200, "", "<html></html>")
        }
    })
    .await;
    let scope = callistseo_core::UrlScope {
        include: vec!["/blog/".into()],
        exclude: vec![r"\.pdf$".into()],
        strip_params: vec!["utm_*".into(), "sid".into()],
        ..Default::default()
    };
    let pages = callistseo_crawler::crawl(start, scoped(scope))
        .await
        .unwrap();
    let mut paths: Vec<String> = pages
        .iter()
        .map(|p| p.url[url::Position::BeforePath..].to_owned())
        .collect();
    paths.sort();
    // L'accueil (départ, jamais filtré), /blog/a une seule fois (paramètres retirés), /blog/b.
    assert_eq!(paths, ["/", "/blog/a", "/blog/b"]);
    let home = pages.iter().find(|p| p.url.path() == "/").unwrap();
    assert!(
        home.links.iter().any(|l| l.path() == "/shop/x"),
        "le lien exclu reste visible dans le graphe"
    );
}

#[tokio::test]
async fn robots_override_replaces_the_site_robots() {
    let start = serve_with(|p, _| match p {
        "/robots.txt" => (200, "", "User-agent: *\nDisallow: /private\n"),
        "/" => (
            200,
            "",
            r#"<a href="/private/a">a</a><a href="/public/b">b</a>"#,
        ),
        _ => (200, "", "<html></html>"),
    })
    .await;
    let blocked = |pages: &[callistseo_core::CrawledPage]| {
        pages.iter().any(|p| {
            p.url.path() == "/private/a"
                && p.error.as_deref() == Some(callistseo_core::ROBOTS_BLOCKED)
        })
    };
    assert!(blocked(
        &callistseo_crawler::crawl(start.clone(), local_cfg())
            .await
            .unwrap()
    ));
    // Le robots.txt personnalisé autorise tout : /private/a est exploré.
    let scope = callistseo_core::UrlScope {
        robots_override: Some("User-agent: *\nAllow: /\n".into()),
        ..Default::default()
    };
    assert!(!blocked(
        &callistseo_crawler::crawl(start, scoped(scope))
            .await
            .unwrap()
    ));
}

#[tokio::test]
async fn an_invalid_scope_fails_before_any_request() {
    let scope = callistseo_core::UrlScope {
        include: vec!["(".into()],
        ..Default::default()
    };
    let err = callistseo_crawler::crawl("http://127.0.0.1:1/".parse().unwrap(), scoped(scope))
        .await
        .unwrap_err();
    assert!(err.contains("inclure"), "{err}");
}

#[tokio::test]
async fn custom_extractors_run_on_every_html_page() {
    use callistseo_core::{ExtractKind, Extractor};
    let start = serve_with(|p, _| match p {
        "/" => (
            200,
            "",
            r#"<script>ga("UA-77")</script><h2 class="t">Accueil</h2><a href="/b">b</a>"#,
        ),
        _ => (200, "", "<h2 class=\"t\">Page B</h2> TODO"),
    })
    .await;
    let ex = |name: &str, kind, pattern: &str| Extractor {
        name: name.into(),
        kind,
        pattern: pattern.into(),
        attr: None,
    };
    let cfg = CrawlConfig {
        extractors: vec![
            ex("ga", ExtractKind::Regex, r"UA-(\d+)"),
            ex("titre", ExtractKind::Css, "h2.t"),
            ex("todo", ExtractKind::Contains, "TODO"),
        ],
        ..local_cfg()
    };
    let pages = callistseo_crawler::crawl(start, cfg).await.unwrap();
    let get = |path: &str, name: &str| {
        pages
            .iter()
            .find(|p| p.url.path() == path)
            .and_then(|p| p.facts.extracted.iter().find(|(n, _)| n == name))
            .map(|(_, v)| v.clone())
    };
    assert_eq!(get("/", "ga"), Some(vec!["77".to_owned()]));
    assert_eq!(get("/", "titre"), Some(vec!["Accueil".to_owned()]));
    assert_eq!(get("/b", "titre"), Some(vec!["Page B".to_owned()]));
    assert_eq!(
        (get("/b", "todo"), get("/", "todo")),
        (Some(vec!["1".to_owned()]), None)
    );
    assert_eq!(get("/b", "ga"), None);
}

#[tokio::test]
async fn an_invalid_extractor_fails_before_any_request() {
    use callistseo_core::{ExtractKind, Extractor};
    let cfg = CrawlConfig {
        extractors: vec![Extractor {
            name: "x".into(),
            kind: ExtractKind::Regex,
            pattern: "(".into(),
            attr: None,
        }],
        ..local_cfg()
    };
    let err = callistseo_crawler::crawl("http://127.0.0.1:1/".parse().unwrap(), cfg)
        .await
        .unwrap_err();
    assert!(err.contains("extraction"), "{err}");
}

// ---- liens externes et ancres ----------------------------------------------------------------

// `localhost` est un autre hôte que `127.0.0.1` : c'est un lien « externe » pour le crawler.
static EXTERNAL_SITE: &[Route] = &[
    (
        "/",
        200,
        "",
        r##"<a href="http://localhost:{PORT}/ok">ok</a><a href="http://localhost:{PORT}/gone">gone</a><a href="http://localhost:{PORT}/denied">denied</a>
<a href="/in"></a><a href="/in2">Click here.</a><a href="/in3"><img src="l.png" alt="logo"></a><a href="#top"></a>"##,
    ),
    ("/in", 200, "", "x"),
    ("/in2", 200, "", "x"),
    ("/in3", 200, "", "x"),
    ("/ok", 200, "", "ok"),
    ("/denied", 403, "", "no"),
];

#[tokio::test]
async fn external_links_are_checked_only_on_request_and_anchors_are_counted() {
    let root = crawl(serve(EXTERNAL_SITE).await, local_cfg())
        .await
        .unwrap()
        .remove(0);
    assert!(root.facts.external_broken.is_empty(), "option désactivée");
    // vide : `/in` ; générique : `/in2` ; `/in3` a un alt ; `#top` est ignoré.
    assert_eq!((root.facts.anchor_empty, root.facts.anchor_generic), (1, 1));

    let cfg = CrawlConfig {
        check_external: true,
        ..local_cfg()
    };
    let root = crawl(serve(EXTERNAL_SITE).await, cfg)
        .await
        .unwrap()
        .remove(0);
    let broken: Vec<_> = root
        .facts
        .external_broken
        .iter()
        .map(|(u, why)| (u.rsplit('/').next().unwrap().to_owned(), why.clone()))
        .collect();
    // 403 = non concluant, 200 = vivant : seul /gone (404) est cassé.
    assert_eq!(broken, [("gone".to_owned(), "404".to_owned())]);
}

static ROBOTS_5XX_SITE: &[Route] = &[
    ("/", 200, "", r#"<a href="/ok">ok</a>"#),
    ("/ok", 200, "", "ok"),
    ("/robots.txt", 503, "", "indisponible"),
];

#[tokio::test]
async fn robots_5xx_blocks_everything_unless_ignored() {
    let start = serve(ROBOTS_5XX_SITE).await;
    let pages = crawl(start.clone(), local_cfg()).await.unwrap();
    assert!(
        pages.iter().all(|p| p.status.is_none()),
        "5xx = tout interdit"
    );

    let cfg = CrawlConfig {
        respect_robots: false,
        ..local_cfg()
    };
    let pages = crawl(start, cfg).await.unwrap();
    assert!(pages
        .iter()
        .any(|p| p.url.path() == "/ok" && p.status == Some(200)));
}

static BASE_HREF_SITE: &[Route] = &[
    ("/", 200, "", r#"<base href="/docs/"><a href="page">p</a>"#),
    ("/docs/page", 200, "", "ok"),
];

#[tokio::test]
async fn base_href_resolves_relative_links() {
    let start = serve(BASE_HREF_SITE).await;
    let pages = crawl(start, local_cfg()).await.unwrap();
    assert!(pages
        .iter()
        .any(|p| p.url.path() == "/docs/page" && p.status == Some(200)));
}

/// Sans `allow_private`, aucune destination interne n'est jamais contactée : IP littérales
/// (métadonnées cloud, loopback, IPv4 mappée) et nom d'hôte résolu vers le loopback.
#[tokio::test]
async fn private_destinations_are_refused_without_allow_private() {
    let local = serve(SITE).await;
    let port = local.port().unwrap();
    for target in [
        "http://169.254.169.254/latest/meta-data/".to_owned(),
        "http://[::1]/".to_owned(),
        "http://[::ffff:127.0.0.1]/".to_owned(),
        "http://10.0.0.1/".to_owned(),
        format!("http://localhost:{port}/"),
        format!("http://127.0.0.1:{port}/"),
    ] {
        let pages = crawl(Url::parse(&target).unwrap(), CrawlConfig::default())
            .await
            .unwrap();
        assert!(
            pages
                .iter()
                .all(|p| p.status.is_none()
                    && p.error.as_ref().is_some_and(|e| e.contains("interdite"))),
            "{target} : {:?}",
            pages
                .iter()
                .map(|p| (&p.status, &p.error))
                .collect::<Vec<_>>()
        );
    }
}

/// Une redirection vers une destination interne n'est pas suivie en place : la page redirigeante
/// est enregistrée avec sa cible, et la cible repasse par la file (donc par le même contrôle).
#[tokio::test]
async fn redirect_to_an_internal_address_is_recorded_not_followed() {
    static R: &[Route] = &[
        ("/", 200, "", r#"<a href="/go">go</a>"#),
        (
            "/go",
            302,
            "Location: http://169.254.169.254/latest/meta-data/\r\n",
            "",
        ),
    ];
    let start = serve(R).await;
    let pages = crawl(start, local_cfg()).await.unwrap();
    let go = pages.iter().find(|p| p.url.path() == "/go").unwrap();
    assert_eq!(go.status, Some(302));
    assert_eq!(
        go.redirect_to.as_ref().unwrap().host_str(),
        Some("169.254.169.254")
    );
}

/// Corpus de pages piégées (`tests/fixtures`, plus des pages générées) : le crawl et l'analyse
/// doivent aller au bout sans planter, et chaque page est enregistrée avec son statut.
#[tokio::test]
async fn hostile_pages_never_crash_the_crawl_or_the_rules() {
    fn leak(s: String) -> &'static str {
        Box::leak(s.into_boxed_str())
    }
    // Plus gros que `max_body_bytes` (5 Mo) : tronqué, jamais lu en entier.
    let huge = leak(format!(
        "<title>énorme</title>{}",
        "<p>mot mot mot <a href=\"/huge.html\">x</a></p>".repeat(150_000)
    ));
    // Imbrication très profonde (pile de l'analyseur HTML).
    let deep = leak(format!(
        "{}<a href=\"/\">fond</a>{}",
        "<div>".repeat(50_000),
        "</div>".repeat(50_000)
    ));
    let routes: &'static [Route] = Box::leak(Box::new([
        (
            "/",
            200,
            "",
            r#"<a href="/malformed.html">1</a><a href="/weird-meta.html">2</a><a href="/empty.html">3</a>
               <a href="/not-html.html">4</a><a href="/huge.html">5</a><a href="/deep.html">6</a>
               <a href="/self-redirect.html">7</a><a href="/bad-location.html">8</a><a href="/gzip-lie.html">9</a>"#,
        ),
        (
            "/malformed.html",
            200,
            "",
            include_str!("fixtures/malformed.html"),
        ),
        (
            "/weird-meta.html",
            200,
            "",
            include_str!("fixtures/weird-meta.html"),
        ),
        ("/empty.html", 200, "", include_str!("fixtures/empty.html")),
        (
            "/not-html.html",
            200,
            "",
            include_str!("fixtures/not-html.html"),
        ),
        ("/huge.html", 200, "", huge),
        ("/deep.html", 200, "", deep),
        (
            "/self-redirect.html",
            301,
            "Location: /self-redirect.html\r\n",
            "",
        ),
        ("/bad-location.html", 302, "Location: ht!tp://[\r\n", ""),
        // Annonce gzip, envoie du texte brut.
        (
            "/gzip-lie.html",
            200,
            "Content-Encoding: gzip\r\n",
            "pas compressé",
        ),
    ]));
    let start = serve(routes).await;
    let pages = crawl(start, local_cfg()).await.unwrap();
    let status = |p: &str| {
        pages
            .iter()
            .find(|x| x.url.path() == p)
            .unwrap_or_else(|| panic!("{p} manquant"))
            .status
    };
    for p in [
        "/malformed.html",
        "/weird-meta.html",
        "/empty.html",
        "/not-html.html",
        "/huge.html",
        "/deep.html",
    ] {
        assert_eq!(status(p), Some(200), "{p}");
    }
    assert_eq!(status("/self-redirect.html"), Some(301));
    assert_eq!(status("/bad-location.html"), Some(302));
    let malformed = pages
        .iter()
        .find(|x| x.url.path() == "/malformed.html")
        .unwrap();
    // Les liens exploitables sont gardés, les autres ignorés sans erreur.
    assert!(malformed.links.iter().any(|l| l.path() == "/espaces.html"));
    assert!(malformed
        .links
        .iter()
        .all(|l| l.scheme() == "http" || l.scheme() == "https"));
    // Les règles passent sur tout le corpus.
    let issues = callistseo_seo_rules::analyze(&pages, &Default::default());
    assert!(!issues.is_empty());
}
