mod adaptive;
mod client;
mod external;
mod extract;
mod login;
mod robots;
mod scope;
mod sitemap;

pub use adaptive::CrawlStats;
pub use client::{is_forbidden, validate_auth, validate_user_agent, Client};
pub use extract::{valid_name as valid_extractor_name, Extractors};
pub use scope::Scope;

use adaptive::Controller;

use callistseo_core::{
    normalize, CrawlConfig, CrawledPage, PageFacts, PageMeta, UrlInterner, MAX_H2, MAX_IMAGES,
    MAX_JSON_LD, MAX_JSON_LD_BYTES, MAX_RESOURCES, SOCIAL_TAGS,
};
use robots::Robots;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::Ordering;
use std::sync::{Arc, LazyLock};
use std::time::Duration;
use tokio::task::JoinSet;
use tokio::time::{sleep_until, Instant};
use url::Url;

/// Robots d'IA d'autres services dont l'interdiction explicite dans robots.txt est signalée, à titre
/// de constat : aucun n'a d'effet sur la Recherche Google (Google-Extended compris).
const AI_BOTS: &[&str] = &[
    "GPTBot",
    "ClaudeBot",
    "PerplexityBot",
    "CCBot",
    "Google-Extended",
    "Applebot-Extended",
];

/// Plafond du délai entre deux requêtes (un `Crawl-delay` démesuré ne doit pas bloquer l'audit).
const MAX_DELAY: Duration = Duration::from_secs(30);
/// Pause imposée par un 429/503 : `Retry-After` borné à cet intervalle (1 s si absent).
const MIN_PAUSE: Duration = Duration::from_secs(1);
const MAX_PAUSE: Duration = Duration::from_secs(60);
/// Une URL qui répond 429/503 est retentée une fois, puis son statut est conservé tel quel.
const MAX_RETRIES: u32 = 1;

/// Explore le site à partir de `start`, en restant sur le même hôte.
/// Les redirections ne sont pas suivies en place : la cible est une nouvelle URL
/// de la file, donc dédupliquée et revalidée comme n'importe quelle autre.
///
/// Politesse : de `cfg.concurrency` à `cfg.max_concurrency` requêtes en vol (adaptatif : monte tant que
/// le serveur est aussi rapide qu'au repos, divise par deux sur 429/5xx/erreur), au moins `max(cfg.delay, Crawl-delay)`
/// entre deux débuts de requête, et suspension générale quand le serveur répond 429/503.
pub async fn crawl(start: Url, cfg: CrawlConfig) -> Result<Vec<CrawledPage>, String> {
    crawl_observed(start, cfg, Arc::default()).await
}

/// Comme `crawl`, avec des statistiques lisibles pendant l'exécution (polling de l'API).
pub async fn crawl_observed(
    start: Url,
    cfg: CrawlConfig,
    stats: Arc<CrawlStats>,
) -> Result<Vec<CrawledPage>, String> {
    crawl_live(start, cfg, stats, None).await
}

/// Comme `crawl_observed`, et chaque page terminée part aussi dans `live` (affichage pendant
/// l'audit). La copie envoyée précède la vérification des liens externes, faite à la fin.
pub async fn crawl_live(
    start: Url,
    cfg: CrawlConfig,
    stats: Arc<CrawlStats>,
    live: Option<tokio::sync::mpsc::UnboundedSender<CrawledPage>>,
) -> Result<Vec<CrawledPage>, String> {
    let mut client = Client::for_site(&cfg, &start)?;
    client.log_in().await?;
    let avoid_logout = client.logged_in();
    let client = Arc::new(client);
    let scope = Scope::new(&cfg.scope)?;
    let extractors = Arc::new(Extractors::new(&cfg.extractors)?);
    let host = start.host_str().map(str::to_owned);
    // Un robots.txt personnalisé remplace celui du site (essai d'une version à venir).
    let robots_txt = match &cfg.scope.robots_override {
        Some(text) => Some(text.clone()),
        None => client.robots_txt(&start).await,
    };
    // Le robots.txt du site, pas un essai personnalisé : seul le vrai fichier dit quoi des robots IA.
    let ai_bots_blocked: Vec<String> = match (&cfg.scope.robots_override, &robots_txt) {
        (None, Some(t)) => AI_BOTS
            .iter()
            .filter(|b| Robots::blocks_named_agent(t, b))
            .map(|b| (*b).to_owned())
            .collect(),
        _ => Vec::new(),
    };
    // Ce que Googlebot a le droit de lire (ressources de rendu), indépendamment de notre user-agent.
    // Un robots.txt d'essai s'applique aussi : on veut savoir s'il bloquerait le CSS ou le JS.
    let googlebot = robots_txt.as_deref().map(|t| Robots::parse(t, "Googlebot"));
    let robots = Arc::new(match (&robots_txt, cfg.respect_robots) {
        (Some(t), true) => Robots::parse(t, &cfg.user_agent),
        _ => Robots::default(),
    });
    // La liste du sitemap n'est pas bornée par max_pages (sinon `in_sitemap` serait faux
    // pour les pages crawlées au-delà) ; 50 000 = maximum du protocole.
    let pending_sitemap =
        collect_sitemap_urls(&client, &start, robots_txt.as_deref(), &host, 50_000).await;
    let mut lastmods: HashMap<Url, String> = HashMap::new();
    let mut pending_sitemap: Vec<Url> = pending_sitemap
        .into_iter()
        .map(|(u, m)| (scope.rewrite(u), m))
        .filter(|(u, _)| scope.allows(u))
        .map(|(u, m)| {
            if let Some(m) = m {
                lastmods.insert(u.clone(), m);
            }
            u
        })
        .collect();
    let in_sitemap: HashSet<Url> = pending_sitemap.iter().cloned().collect();
    let robots_delay = if cfg.respect_robots {
        robots.crawl_delay()
    } else {
        None
    };
    let delay = cfg
        .delay
        .max(robots_delay.unwrap_or_default())
        .min(MAX_DELAY);
    // Un délai impose déjà le rythme (une requête à la fois par créneau) : concurrence fixe.
    let mut ctl = if delay.is_zero() {
        Controller::new(cfg.concurrency, cfg.max_concurrency)
    } else {
        Controller::new(cfg.concurrency, cfg.concurrency)
    };
    stats.limit.store(ctl.limit(), Ordering::Relaxed);
    stats.max_pages.store(cfg.max_pages, Ordering::Relaxed);
    let mut next_slot = Instant::now();
    let mut retries: HashMap<Url, u32> = HashMap::new();
    // Une seule allocation par URL distincte, partagée par `seen`, la file et les liens des pages.
    let mut interner = UrlInterner::default();
    let start_url = start.clone();
    let start = interner.intern(start);
    let mut seen: HashSet<Arc<Url>> = HashSet::from([start.clone()]);
    let mut queue: VecDeque<(Arc<Url>, usize)> = VecDeque::from([(start, 0)]);
    let mut tasks = JoinSet::new();
    let mut pages = Vec::new();

    loop {
        if stats.stop.load(Ordering::Relaxed) {
            // Arrêt demandé : plus rien n'est lancé, les requêtes en vol se terminent, puis on rend ce qu'on a.
            queue.clear();
            pending_sitemap.clear();
        }
        // En pause (et rien en vol) : on attend la reprise ou l'arrêt.
        while tasks.is_empty()
            && stats.paused.load(Ordering::Relaxed)
            && !stats.stop.load(Ordering::Relaxed)
        {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        while tasks.len() < ctl.limit() && !stats.paused.load(Ordering::Relaxed) {
            let Some((url, depth)) = queue.pop_front() else {
                break;
            };
            // Limiteur : une requête ne démarre pas avant `next_slot` (délai, ou pause après un 429).
            let start_at = next_slot.max(Instant::now());
            if start_at > Instant::now() {
                sleep_until(start_at).await;
            }
            next_slot = start_at + delay;
            let (client, robots, extractors) = (client.clone(), robots.clone(), extractors.clone());
            let url = (*url).clone();
            tasks.spawn(async move { visit(&client, &robots, extractors, url, depth).await });
        }
        stats.in_flight.store(tasks.len(), Ordering::Relaxed);
        stats.queued.store(queue.len(), Ordering::Relaxed);
        if tasks.is_empty() {
            // Parcours terminé : on ajoute les URL de sitemap jamais rencontrées via un lien.
            for u in std::mem::take(&mut pending_sitemap) {
                let u = interner.intern(u);
                if seen.len() < cfg.max_pages && seen.insert(u.clone()) {
                    queue.push_back((u, 1));
                }
            }
            if queue.is_empty() {
                break;
            }
            continue;
        }
        let Some(done) = tasks.join_next().await else {
            break;
        };
        let Visited {
            mut page,
            links,
            retry_after,
            timing,
        } = done.map_err(|e| e.to_string())?;
        if let Some((latency, bad)) = timing {
            stats
                .limit
                .store(ctl.record(latency, bad), Ordering::Relaxed);
            // Moyenne mobile exponentielle (poids 0,1) de la latence.
            let us = latency.as_micros() as u64;
            let avg = stats.avg_latency_us.load(Ordering::Relaxed);
            stats.avg_latency_us.store(
                if avg == 0 { us } else { (avg * 9 + us) / 10 },
                Ordering::Relaxed,
            );
            if bad {
                stats.failures.fetch_add(1, Ordering::Relaxed);
            }
        }
        if let Some(wait) = retry_after {
            // Le serveur demande de ralentir : on suspend tout le crawl, puis on retente une fois.
            let tries = retries.entry(page.url.clone()).or_default();
            if *tries < MAX_RETRIES {
                *tries += 1;
                next_slot = next_slot.max(Instant::now() + wait.clamp(MIN_PAUSE, MAX_PAUSE));
                queue.push_front((interner.intern(page.url.clone()), page.depth));
                continue;
            }
        }
        page.in_sitemap = in_sitemap.contains(&page.url);
        page.facts.sitemap_lastmod = lastmods.get(&page.url).cloned();
        if page.url == start_url {
            page.facts.ai_bots_blocked = ai_bots_blocked.clone();
        }
        match &googlebot {
            Some(g) => page.facts.blocked_resources.retain(|r| {
                Url::parse(r).is_ok_and(|u| {
                    !g.allowed(&u[url::Position::BeforePath..url::Position::AfterQuery])
                })
            }),
            None => page.facts.blocked_resources.clear(),
        }
        page.facts.blocked_resources.truncate(MAX_RESOURCES);

        // Vec explicite, volontairement : `into_iter().map(..).collect()` réutiliserait (« in-place
        // collect ») le gros tampon de `Vec<Url>` (88 o par élément) pour des `Arc<Url>` de 8 o, et
        // chaque page garderait ≈ 45 Ko de capacité inutilisée (mesuré avec dhat : 85 % du tas).
        let mut links: Vec<Url> = links.into_iter().map(|u| scope.rewrite(u)).collect();
        links.sort();
        links.dedup();
        let mut interned = Vec::with_capacity(links.len());
        interned.extend(links.into_iter().map(|u| interner.intern(u)));
        page.links = interned;

        let redirect = page.redirect_to.clone().map(|r| interner.intern(r));
        let next = page
            .links
            .iter()
            .map(|l| (l, page.depth + 1))
            .chain(redirect.iter().map(|r| (r, page.depth)));
        for (link, depth) in next {
            // Tests peu coûteux d'abord ; aucune allocation (comparaison d'hôtes par référence).
            if depth <= cfg.max_depth
                && link.host_str() == host.as_deref()
                && !is_infrastructure(link)
                && scope.allows(link)
                && !(avoid_logout && login::is_logout(link))
                && seen.len() < cfg.max_pages
                && seen.insert(link.clone())
            {
                queue.push_back((link.clone(), depth));
            }
        }
        if let Some(tx) = &live {
            // Récepteur fermé (audit annulé) : sans conséquence pour le crawl.
            let _ = tx.send(page.clone());
        }
        pages.push(page);
        stats.completed.store(pages.len(), Ordering::Relaxed);
    }
    if cfg.check_external && !stats.stop.load(Ordering::Relaxed) {
        external::check(&client, &cfg, host.as_deref(), &mut pages).await;
    }
    Ok(pages)
}

/// URL d'infrastructure du CDN, jamais de vraies pages : Cloudflare réécrit les `mailto:` en
/// `/cdn-cgi/l/email-protection#…`, qui répond 404 sans son fragment (faux « lien cassé »).
fn is_infrastructure(url: &Url) -> bool {
    url.path().starts_with("/cdn-cgi/")
}

/// URL listées dans les sitemaps (déclarés dans robots.txt, sinon `/sitemap.xml`), même hôte.
async fn collect_sitemap_urls(
    client: &Client,
    start: &Url,
    robots_txt: Option<&str>,
    host: &Option<String>,
    limit: usize,
) -> Vec<(Url, Option<String>)> {
    const MAX_FILES: usize = 20;
    let mut roots: Vec<Url> = robots_txt
        .map(sitemap::declared)
        .unwrap_or_default()
        .iter()
        .filter_map(|u| normalize(u, None))
        .collect();
    if roots.is_empty() {
        roots.extend(start.join("/sitemap.xml"));
    }
    let mut todo: VecDeque<(Url, usize)> = roots.into_iter().map(|u| (u, 0)).collect();
    let (mut files, mut urls) = (HashSet::new(), Vec::new());
    while let Some((u, level)) = todo.pop_front() {
        if files.len() >= MAX_FILES || !files.insert(u.clone()) {
            continue;
        }
        let Some(xml) = client.get_text(&u, 50 * 1024 * 1024).await else {
            continue;
        };
        let (is_index, locs) = sitemap::parse_entries(&xml);
        for (loc, lastmod) in locs
            .into_iter()
            .filter_map(|(l, m)| Some((normalize(&l, None)?, m)))
        {
            if is_index {
                if level < 2 {
                    todo.push_back((loc, level + 1));
                }
            } else if loc.host_str() == host.as_deref() && urls.len() < limit {
                urls.push((loc, lastmod));
            }
        }
    }
    urls
}

struct Visited {
    page: CrawledPage,
    /// Liens trouvés dans la page (internés par le coordinateur, pas ici).
    links: Vec<Url>,
    /// Pause demandée par le serveur (429/503), à respecter avant de retenter.
    retry_after: Option<Duration>,
    /// (durée de la requête, en échec ?) ; `None` si aucune requête n'a eu lieu (robots.txt).
    timing: Option<(Duration, bool)>,
}

async fn visit(
    client: &Client,
    robots: &Robots,
    extractors: Arc<Extractors>,
    url: Url,
    depth: usize,
) -> Visited {
    let mut page = CrawledPage {
        facts: Default::default(),
        in_sitemap: false,
        is_html: false,
        meta: PageMeta::default(),
        url: url.clone(),
        depth,
        status: None,
        redirect_to: None,
        links: vec![],
        error: None,
    };
    let path = &url[url::Position::BeforePath..url::Position::AfterQuery];
    if !robots.allowed(path) {
        page.error = Some(callistseo_core::ROBOTS_BLOCKED.into());
        return Visited {
            page,
            links: Vec::new(),
            retry_after: None,
            timing: None,
        };
    }
    let mut retry_after = None;
    let mut links = Vec::new();
    let started = std::time::Instant::now();
    let fetched = client.fetch(&url).await;
    let elapsed = started.elapsed();
    // « En échec » pour le contrôleur : erreur réseau, 429 ou 5xx (le serveur est sous tension).
    let bad = match &fetched {
        Err(_) => true,
        Ok(f) => f.status == 429 || f.status >= 500,
    };
    match fetched {
        Err(e) => page.error = Some(e),
        Ok(f) => {
            page.status = Some(f.status);
            if matches!(f.status, 429 | 503) {
                retry_after = Some(f.retry_after.unwrap_or(MIN_PAUSE));
            }
            page.redirect_to = f.location.and_then(|l| normalize(&l, Some(&url)));
            page.facts = PageFacts {
                ttfb_ms: Some(f.ttfb.as_millis() as u32),
                total_ms: Some(f.total.as_millis() as u32),
                size: f.size,
                content_type: f.content_type,
                headers: f.headers,
                ..PageFacts::default()
            };
            if let Some(body) = f.html {
                page.is_html = true;
                // Le parsing est du CPU pur : hors du runtime pour ne pas bloquer les sockets.
                let (base, mut facts) = (url.clone(), std::mem::take(&mut page.facts));
                let parsed = tokio::task::spawn_blocking(move || {
                    let (links, meta) = parse_html(&body, &base, &extractors, &mut facts);
                    (links, meta, facts)
                })
                .await;
                match parsed {
                    Ok((l, meta, facts)) => {
                        (links, page.meta, page.facts) = (l, meta, facts);
                    }
                    Err(e) => page.error = Some(format!("analyse HTML interrompue : {e}")),
                }
            }
        }
    }
    Visited {
        page,
        links,
        retry_after,
        timing: Some((elapsed, bad)),
    }
}

// ponytail: rel=nofollow n'empêche pas l'exploration (le lien reste dans le graphe) ; un seul parse pour liens + métadonnées.
/// Sélecteurs CSS compilés une seule fois (ils l'étaient à chaque page, plusieurs fois par page).
struct Selectors {
    anchors: scraper::Selector,
    base: scraper::Selector,
    title: scraper::Selector,
    meta: scraper::Selector,
    h1: scraper::Selector,
    canonical: scraper::Selector,
    h2: scraper::Selector,
    hreflang: scraper::Selector,
    images: scraper::Selector,
    json_ld: scraper::Selector,
    amp_html: scraper::Selector,
    itemscope: scraper::Selector,
    head_title: scraper::Selector,
    social: scraper::Selector,
    icon: scraper::Selector,
    resources: scraper::Selector,
}

static SELECTORS: LazyLock<Selectors> = LazyLock::new(|| {
    let parse = |s| scraper::Selector::parse(s).expect("sélecteur constant valide");
    Selectors {
        anchors: parse("a[href]"),
        base: parse("base[href]"),
        title: parse("title"),
        meta: parse("meta[name][content]"),
        h1: parse("h1"),
        canonical: parse("link[rel~=canonical][href]"),
        h2: parse("h2"),
        hreflang: parse("link[rel~=alternate][hreflang][href]"),
        images: parse("img"),
        json_ld: parse(r#"script[type="application/ld+json"]"#),
        amp_html: parse("link[rel~=amphtml][href]"),
        itemscope: parse("[itemscope]"),
        // Dans `<head>` seulement : un `<title>` d'icône SVG dans le corps n'en est pas un.
        head_title: parse("head title"),
        social: parse("meta[property][content], meta[name][content]"),
        icon: parse("link[rel~=icon][href]"),
        resources: parse("link[rel~=stylesheet][href], script[src]"),
    }
});

/// Texte visible normalisé (espaces réduits, sans script/style) : nombre de mots, empreinte
/// FNV-1a 64 bits (doublons exacts) et simhash 64 bits sur les groupes de 3 mots (quasi-doublons).
fn text_facts(doc: &scraper::Html) -> (u32, String, String) {
    let fnv = |bytes: &[&[u8]]| {
        bytes
            .iter()
            .flat_map(|b| b.iter())
            .fold(0xcbf29ce484222325u64, |h, &b| {
                (h ^ u64::from(b)).wrapping_mul(0x100000001b3)
            })
    };
    let mut all = Vec::new();
    for node in doc.tree.nodes() {
        let scraper::Node::Text(t) = node.value() else {
            continue;
        };
        let hidden = node.parent().is_some_and(|p| {
            matches!(
                p.value().as_element().map(|e| e.name()),
                Some("script" | "style" | "noscript" | "template" | "title")
            )
        });
        if !hidden {
            all.extend(t.split_whitespace());
        }
    }
    let mut hash = 0xcbf29ce484222325u64;
    for (i, w) in all.iter().enumerate() {
        if i > 0 {
            hash = (hash ^ u64::from(b' ')).wrapping_mul(0x100000001b3);
        }
        for b in w.bytes() {
            hash = (hash ^ u64::from(b)).wrapping_mul(0x100000001b3);
        }
    }
    // ponytail: simhash non pondéré ; ajouter des poids (tf-idf) si les faux positifs gênent.
    let mut acc = [0i32; 64];
    let shingles = all.windows(3).map(|w| {
        fnv(&[
            w[0].as_bytes(),
            b" ",
            w[1].as_bytes(),
            b" ",
            w[2].as_bytes(),
        ])
    });
    for h in shingles {
        for (bit, a) in acc.iter_mut().enumerate() {
            *a += if h >> bit & 1 == 1 { 1 } else { -1 };
        }
    }
    let sim = acc
        .iter()
        .enumerate()
        .fold(0u64, |s, (bit, &a)| if a > 0 { s | 1 << bit } else { s });
    (
        all.len() as u32,
        format!("{hash:016x}"),
        format!("{sim:016x}"),
    )
}

/// Ancres qui ne disent rien de la page visée (comparaison exacte, sans casse ni ponctuation finale).
fn is_generic_anchor(text: &str) -> bool {
    const GENERIC: &[&str] = &[
        "ici",
        "cliquez ici",
        "clic ici",
        "en savoir plus",
        "lire la suite",
        "lire plus",
        "voir plus",
        "plus",
        "suite",
        "click here",
        "here",
        "read more",
        "learn more",
        "more",
        "link",
    ];
    let t = text
        .trim()
        .trim_end_matches(['.', '…', '>', '»', ' '])
        .to_lowercase();
    GENERIC.contains(&t.as_str())
}

/// Au-delà, le document est coupé avant l'analyse : le temps de l'analyseur HTML croît avec
/// (balises × profondeur), soit 8 s pour 100 000 `<div>` imbriquées. Chrome s'arrête à 512 niveaux ;
/// les vraies pages restent sous 100.
pub(crate) const MAX_NESTING: usize = 1_000;

/// Début de `html` jusqu'à la balise qui dépasserait `max` niveaux d'imbrication (tout le document
/// sinon). Estimation rapide sur le texte : les éléments vides et ceux que l'analyseur ferme tout
/// seuls (`<p>`, `<li>`…) ne comptent pas, pour ne pas couper du HTML ancien mais valide.
// ponytail: pas de tokenizer complet (commentaires et scripts comptent comme du balisage) ; sans effet sous la limite.
pub(crate) fn cap_nesting(html: &str, max: usize) -> &str {
    const FLAT: &[&str] = &[
        "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source",
        "track", "wbr", "p", "li", "dt", "dd", "option", "tr", "td", "th", "thead", "tbody",
        "tfoot", "colgroup",
    ];
    let bytes = html.as_bytes();
    let mut depth = 0usize;
    let mut i = 0;
    while let Some(off) = bytes[i..].iter().position(|&b| b == b'<') {
        let at = i + off;
        i = at + 1;
        let closing = bytes.get(i) == Some(&b'/');
        let name_start = i + usize::from(closing);
        // Doctype, commentaires, `<?…>`, `<` isolé : pas un élément.
        if !bytes.get(name_start).is_some_and(u8::is_ascii_alphabetic) {
            continue;
        }
        let name_len = bytes[name_start..]
            .iter()
            .take_while(|b| b.is_ascii_alphanumeric())
            .count();
        let name = html[name_start..name_start + name_len].to_ascii_lowercase();
        if FLAT.contains(&name.as_str()) {
            continue;
        }
        if closing {
            depth = depth.saturating_sub(1);
            continue;
        }
        let end = bytes[i..]
            .iter()
            .position(|&b| b == b'>')
            .map_or(bytes.len(), |e| i + e);
        if end > 0 && bytes[end - 1] == b'/' {
            continue; // `<x/>`
        }
        depth += 1;
        if depth > max {
            return &html[..at];
        }
    }
    html
}

fn parse_html(
    body: &[u8],
    base: &Url,
    extractors: &Extractors,
    facts: &mut PageFacts,
) -> (Vec<Url>, PageMeta) {
    let page_host = base.host_str().map(str::to_owned);
    let source = String::from_utf8_lossy(body);
    let doc = scraper::Html::parse_document(cap_nesting(&source, MAX_NESTING));
    let sel = &*SELECTORS;
    // `<base href>` (premier élément) change la base de résolution des URL relatives.
    let doc_base = doc
        .select(&sel.base)
        .next()
        .and_then(|b| normalize(b.value().attr("href")?.trim(), Some(base)));
    let base = doc_base.as_ref().unwrap_or(base);
    let text = |e: scraper::ElementRef| e.text().collect::<String>().trim().to_owned();
    let meta_content = |name: &str| {
        doc.select(&sel.meta)
            .find(|m| {
                m.value()
                    .attr("name")
                    .is_some_and(|n| n.eq_ignore_ascii_case(name))
            })
            .map(|m| {
                m.value()
                    .attr("content")
                    .unwrap_or_default()
                    .trim()
                    .to_owned()
            })
    };

    let mut links: Vec<Url> = doc
        .select(&sel.anchors)
        .filter_map(|a| normalize(a.value().attr("href")?.trim(), Some(base)))
        .collect();
    (facts.anchor_empty, facts.anchor_generic) = (0, 0);
    for a in doc.select(&sel.anchors) {
        let href = a.value().attr("href").unwrap_or_default().trim();
        if href.starts_with('#') || href.to_lowercase().starts_with("javascript:") {
            continue;
        }
        let attr = |n| a.value().attr(n).map(str::trim).unwrap_or_default();
        let mut label = a.text().collect::<Vec<_>>().join(" ");
        for img in a.select(&sel.images) {
            label.push(' ');
            label.push_str(img.value().attr("alt").unwrap_or_default());
        }
        let label = [label.as_str(), attr("aria-label"), attr("title")]
            .iter()
            .map(|s| s.split_whitespace().collect::<Vec<_>>().join(" "))
            .find(|s| !s.is_empty());
        match label {
            None => facts.anchor_empty += 1,
            Some(l) if is_generic_anchor(&l) => facts.anchor_generic += 1,
            _ => {}
        }
    }
    facts.nofollow_links = doc
        .select(&sel.anchors)
        .filter(|a| {
            a.value().attr("rel").is_some_and(|r| {
                r.split_whitespace()
                    .any(|t| t.eq_ignore_ascii_case("nofollow"))
            })
        })
        .count() as u32;
    links.sort();
    links.dedup();

    let meta = PageMeta {
        title: doc.select(&sel.title).next().map(text),
        description: meta_content("description"),
        h1: doc.select(&sel.h1).map(text).collect(),
        canonical: doc
            .select(&sel.canonical)
            .next()
            .and_then(|l| normalize(l.value().attr("href")?.trim(), Some(base))),
        robots: meta_content("robots").map(|r| r.to_lowercase()),
    };

    let (words, hash, sim) = text_facts(&doc);
    facts.word_count = Some(words);
    facts.content_hash = Some(hash);
    facts.simhash = Some(sim);
    facts.lang = doc
        .root_element()
        .value()
        .attr("lang")
        .map(|l| l.trim().to_owned())
        .filter(|l| !l.is_empty());
    facts.is_amp = doc
        .root_element()
        .value()
        .attrs()
        .any(|(n, _)| n == "amp" || n == "\u{26a1}" || n == "\u{26a1}\u{fe0f}");
    facts.amp_html = doc
        .select(&sel.amp_html)
        .next()
        .and_then(|l| normalize(l.value().attr("href")?.trim(), Some(base)))
        .map(|u| u.to_string());
    facts.microdata = 0;
    facts.microdata_untyped = 0;
    for e in doc.select(&sel.itemscope) {
        facts.microdata += 1;
        if e.value()
            .attr("itemtype")
            .is_none_or(|t| t.trim().is_empty())
        {
            facts.microdata_untyped += 1;
        }
    }
    facts.viewport = meta_content("viewport");
    facts.h2 = doc.select(&sel.h2).take(MAX_H2).map(text).collect();
    facts.hreflang = doc
        .select(&sel.hreflang)
        .filter_map(|l| {
            let lang = l.value().attr("hreflang")?.trim().to_lowercase();
            let href = normalize(l.value().attr("href")?.trim(), Some(base))?;
            Some((lang, href.to_string()))
        })
        .collect();
    facts.images_total = 0;
    facts.images.clear();
    for img in doc.select(&sel.images) {
        facts.images_total += 1;
        if facts.images.len() < MAX_IMAGES {
            let src = img.value().attr("src").unwrap_or_default().trim();
            facts
                .images
                .push((src.to_owned(), img.value().attr("alt").map(str::to_owned)));
        }
    }
    facts.json_ld = doc
        .select(&sel.json_ld)
        .take(MAX_JSON_LD)
        .map(|s| {
            s.text()
                .collect::<String>()
                .chars()
                .take(MAX_JSON_LD_BYTES)
                .collect()
        })
        .collect();
    // Balises en double : Google peut ignorer des canonicals contradictoires.
    facts.title_tags = Some(doc.select(&sel.head_title).count() as u32);
    facts.description_tags = Some(
        doc.select(&sel.meta)
            .filter(|m| {
                m.value()
                    .attr("name")
                    .is_some_and(|n| n.eq_ignore_ascii_case("description"))
            })
            .count() as u32,
    );
    facts.canonical_tags = Some(doc.select(&sel.canonical).count() as u32);
    facts.images_unsized = Some(
        doc.select(&sel.images)
            .filter(|i| i.value().attr("width").is_none() || i.value().attr("height").is_none())
            .count() as u32,
    );
    facts.social = SOCIAL_TAGS
        .iter()
        .filter(|tag| {
            doc.select(&sel.social).any(|m| {
                let v = m.value();
                let key = v
                    .attr("property")
                    .or_else(|| v.attr("name"))
                    .unwrap_or_default();
                key.trim().eq_ignore_ascii_case(tag)
                    && !v.attr("content").unwrap_or_default().trim().is_empty()
            })
        })
        .map(|t| t.to_string())
        .collect();
    facts.icon = Some(doc.select(&sel.icon).next().is_some());
    // Candidats seulement : la boucle du crawl ne garde que ceux que robots.txt interdit à Googlebot.
    facts.blocked_resources = doc
        .select(&sel.resources)
        .filter_map(|e| {
            let v = e.value();
            normalize(v.attr("href").or_else(|| v.attr("src"))?.trim(), Some(base))
        })
        .filter(|u| u.host_str() == page_host.as_deref())
        .map(|u| u.to_string())
        .take(MAX_RESOURCES * 5)
        .collect();
    facts.blocked_resources.sort();
    facts.blocked_resources.dedup();
    if !extractors.is_empty() {
        facts.extracted = extractors.run(&source, &doc);
    }
    (links, meta)
}

#[cfg(test)]
mod tests {
    #[test]
    fn deep_nesting_is_cut_but_ordinary_html_is_kept() {
        use super::cap_nesting;
        let deep = format!("<html>{}x{}", "<div>".repeat(5), "</div>".repeat(5));
        assert_eq!(cap_nesting(&deep, 3), "<html><div><div>");
        // Profondeur qui redescend, éléments vides, auto-fermés et `<p>` / `<li>` non fermés : intact.
        let flat = format!(
            "<!doctype html><!-- note --><ul>{}</ul>{}<img src=x><br/><x-a/>a < b",
            "<li><p>texte".repeat(50),
            "<div><span>x</span></div>".repeat(50)
        );
        assert_eq!(cap_nesting(&flat, 3), flat);
        // Coupe sur une frontière de caractère, même avec du texte non ASCII.
        assert_eq!(cap_nesting("<a>é<b>è<c>", 2), "<a>é<b>è");
    }

    use super::*;

    #[test]
    fn extracts_links_and_meta() {
        let base = Url::parse("http://a.test/dir/").unwrap();
        let html = br##"<html><head><title> Hi </title><meta name="Description" content="d">
            <meta name="robots" content="NOINDEX, follow"><link rel="canonical" href="/c#x"></head>
            <body><h1>One</h1><h1>Two</h1><a href="x">1</a><a href="x#top">2</a>
            <a href="mailto:a@b.c">3</a><a>4</a><a href="/y?q=1">5</a></body></html>"##;
        let mut facts = PageFacts::default();
        let (links, m) = parse_html(html, &base, &Extractors::default(), &mut facts);
        let links: Vec<_> = links.iter().map(Url::to_string).collect();
        assert_eq!(links, ["http://a.test/dir/x", "http://a.test/y?q=1"]);
        assert_eq!(m.title.as_deref(), Some("Hi"));
        assert_eq!(m.description.as_deref(), Some("d"));
        assert_eq!(m.h1, ["One", "Two"]);
        assert_eq!(m.canonical.unwrap().as_str(), "http://a.test/c");
        assert_eq!(m.robots.as_deref(), Some("noindex, follow"));
    }

    #[test]
    fn missing_meta_is_none() {
        let (_, m) = parse_html(
            b"<p>x</p>",
            &Url::parse("http://a.test/").unwrap(),
            &Extractors::default(),
            &mut PageFacts::default(),
        );
        assert!(
            m.title.is_none()
                && m.description.is_none()
                && m.h1.is_empty()
                && m.canonical.is_none()
        );
    }

    #[test]
    fn counts_head_tags_social_and_icon() {
        let html = br#"<html><head><title>A</title><title>B</title>
            <meta name="description" content="x"><meta name="Description" content="y">
            <link rel="canonical" href="/a"><link rel="canonical" href="/b">
            <meta property="og:title" content="T"><meta property="og:image" content="  ">
            <meta name="twitter:card" content="summary"><link rel="shortcut icon" href="/f.ico"></head>
            <body><svg><title>icon</title></svg><img src="a" width="10" height="10"><img src="b" width="10"></body></html>"#;
        let mut f = PageFacts::default();
        parse_html(
            html,
            &Url::parse("http://a.test/").unwrap(),
            &Extractors::default(),
            &mut f,
        );
        // Le <title> du SVG (corps) ne compte pas ; og:image vide non plus.
        assert_eq!(
            (f.title_tags, f.description_tags, f.canonical_tags),
            (Some(2), Some(2), Some(2))
        );
        assert_eq!(f.images_unsized, Some(1));
        assert_eq!(f.social, ["og:title", "twitter:card"]);
        assert_eq!(f.icon, Some(true));
        // Pas de CSS ni de script dans ce document.
        assert!(f.blocked_resources.is_empty());
    }

    #[test]
    fn extracts_facts() {
        let base = Url::parse("http://a.test/p/").unwrap();
        let html = br#"<html lang=" fr-FR "><head><meta name="viewport" content="width=device-width">
            <link rel="alternate" hreflang="EN" href="/en/">
            <script type="application/ld+json">{"@type":"Article"}</script>
            <style>p { color: red }</style></head>
            <body><h2>A</h2><h2>B</h2><img src="a.png" alt="x"><img src="b.png" alt=""><img src="c.png">
            <p>Un  deux
               trois</p><script>var ignored = 1</script>
            <a href="/x" rel="Nofollow noopener">x</a><a href="/y">y</a></body></html>"#;
        let mut f = PageFacts::default();
        parse_html(html, &base, &Extractors::default(), &mut f);
        assert_eq!(f.lang.as_deref(), Some("fr-FR"));
        assert_eq!(f.viewport.as_deref(), Some("width=device-width"));
        assert_eq!(f.h2, ["A", "B"]);
        assert_eq!(
            f.hreflang,
            [("en".to_owned(), "http://a.test/en/".to_owned())]
        );
        assert_eq!(f.images_total, 3);
        assert_eq!(
            f.images,
            [
                ("a.png".to_owned(), Some("x".to_owned())),
                ("b.png".to_owned(), Some(String::new())),
                ("c.png".to_owned(), None)
            ]
        );
        assert_eq!(f.json_ld, [r#"{"@type":"Article"}"#]);
        assert_eq!(f.nofollow_links, 1);
        // Schéma v16 : rien de tout cela dans ce document, mais c'est mesuré.
        assert_eq!(
            (
                f.title_tags,
                f.description_tags,
                f.canonical_tags,
                f.images_unsized,
                f.icon
            ),
            (Some(0), Some(0), Some(0), Some(3), Some(false))
        );
        assert!(f.social.is_empty());
        // Mots : A B Un deux trois x y (script/style exclus).
        assert_eq!(f.word_count, Some(7));
    }

    #[test]
    fn content_hash_ignores_markup_and_spacing_but_not_words() {
        let hash = |html: &str| {
            let mut f = PageFacts::default();
            parse_html(
                html.as_bytes(),
                &Url::parse("http://a.test/").unwrap(),
                &Extractors::default(),
                &mut f,
            );
            f.content_hash.unwrap()
        };
        let a = hash("<p>Bonjour   le <b>monde</b></p>");
        assert_eq!(a, hash("<div>Bonjour le\n monde</div><script>x()</script>"));
        assert_ne!(a, hash("<p>Bonjour le monde !</p>"));
    }
}
