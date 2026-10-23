//! Vérification (optionnelle) des liens sortants vers d'autres hôtes.
//!
//! Politesse : un hôte à la fois par requête (séquentiel), au moins `MIN_GAP` entre deux requêtes
//! au même hôte (ou `cfg.delay` / `Crawl-delay` s'ils sont plus grands), robots.txt de l'hôte
//! respecté, au plus `MAX_URLS` URL distinctes et `MAX_HOSTS` hôtes en parallèle. Les
//! redirections ne sont pas suivies : une 3xx compte comme un lien vivant.

use crate::{client::Client, robots::Robots, MAX_DELAY};
use callistseo_core::{CrawlConfig, CrawledPage};
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinSet;
use url::Url;

const MAX_URLS: usize = 500;
const MAX_HOSTS: usize = 8;
const MIN_GAP: Duration = Duration::from_millis(500);

/// Statuts qui ne prouvent pas un lien cassé (blocage anti-robot, limitation de débit).
fn inconclusive(status: u16) -> bool {
    matches!(status, 401 | 403 | 429 | 999)
}

/// Remplit `facts.external_broken` des pages qui pointent vers un lien externe en échec.
pub(crate) async fn check(
    client: &Arc<Client>,
    cfg: &CrawlConfig,
    host: Option<&str>,
    pages: &mut [CrawledPage],
) {
    // Hôte → URL distinctes (ordre stable), plafonnées au total.
    let mut by_host: BTreeMap<String, Vec<Url>> = BTreeMap::new();
    let mut total = 0;
    for l in pages.iter().flat_map(|p| p.links.iter()) {
        let Some(h) = l.host_str() else { continue };
        if !matches!(l.scheme(), "http" | "https") || Some(h) == host {
            continue;
        }
        if total >= MAX_URLS {
            break;
        }
        let urls = by_host.entry(h.to_owned()).or_default();
        let mut u = (**l).clone();
        u.set_fragment(None);
        if !urls.contains(&u) {
            urls.push(u);
            total += 1;
        }
    }

    let mut results: HashMap<String, String> = HashMap::new();
    let mut tasks = JoinSet::new();
    let mut hosts = by_host.into_iter();
    loop {
        while tasks.len() < MAX_HOSTS {
            let Some((_, urls)) = hosts.next() else { break };
            let (client, ua, delay) = (client.clone(), cfg.user_agent.clone(), cfg.delay);
            tasks.spawn(async move { check_host(&client, &ua, delay, urls).await });
        }
        let Some(done) = tasks.join_next().await else {
            break;
        };
        results.extend(done.unwrap_or_default());
    }

    for p in pages.iter_mut() {
        let mut broken: Vec<(String, String)> = p
            .links
            .iter()
            .filter_map(|l| Some((l.to_string(), results.get(l.as_str())?.clone())))
            .collect();
        broken.dedup();
        p.facts.external_broken = broken;
    }
}

/// URL en échec d'un hôte → raison (« 404 » ou message réseau).
async fn check_host(
    client: &Client,
    ua: &str,
    delay: Duration,
    urls: Vec<Url>,
) -> Vec<(String, String)> {
    let robots = match client.robots_txt(&urls[0]).await {
        Some(t) => Robots::parse(&t, ua),
        None => Robots::default(),
    };
    let gap = delay
        .max(robots.crawl_delay().unwrap_or_default())
        .max(MIN_GAP)
        .min(MAX_DELAY);
    let mut broken = Vec::new();
    for (i, u) in urls.iter().enumerate() {
        if !robots.allowed(&u[url::Position::BeforePath..url::Position::AfterQuery]) {
            continue;
        }
        if i > 0 {
            tokio::time::sleep(gap).await;
        }
        // ponytail: GET (lit au plus `max_body`) ; certains serveurs refusent HEAD. Passer en HEAD si le coût gêne.
        match client.fetch(u).await {
            Ok(f) if f.status < 400 || inconclusive(f.status) => {}
            Ok(f) => broken.push((u.to_string(), f.status.to_string())),
            Err(e) => broken.push((u.to_string(), e)),
        }
    }
    broken
}
