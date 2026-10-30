//! Site synthétique déterministe : mêmes pages, mêmes défauts et mêmes comptes attendus à chaque
//! exécution. Les pages sont fabriquées à la demande (aucun fichier), donc le serveur est léger et
//! le même corpus peut être reproduit n'importe où (`callistseo-bench serve`).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

// Défauts injectés (chaque règle a une période propre, sans recouvrement gênant).
pub fn missing_title(i: usize) -> bool {
    i % 50 == 7
}
pub fn duplicate_title(i: usize) -> bool {
    i % 25 == 3
}
pub fn missing_description(i: usize) -> bool {
    i.is_multiple_of(10)
}
pub fn missing_h1(i: usize) -> bool {
    i % 30 == 5
}
pub fn broken_link(i: usize) -> bool {
    i % 40 == 11
}

// Défauts de la phase 3 (règles sur les ancres, JSON-LD, microdata, AMP, quasi-doublons).
pub fn empty_anchor(i: usize) -> bool {
    i % 60 == 13
}
pub fn generic_anchor(i: usize) -> bool {
    i % 45 == 17
}
pub fn json_ld_incomplete(i: usize) -> bool {
    i % 70 == 19
}
pub fn untyped_microdata(i: usize) -> bool {
    i % 80 == 21
}
pub fn amp_without_canonical(i: usize) -> bool {
    i % 90 == 23
}
pub fn no_snippet(i: usize) -> bool {
    i % 110 == 31
}
pub fn faq_schema(i: usize) -> bool {
    i % 120 == 37
}
// Défauts des règles ajoutées ensuite (balises, images, nofollow, ressources, fausses 404).
pub fn doubled_title(i: usize) -> bool {
    i % 130 == 41
}
pub fn unsized_image(i: usize) -> bool {
    i % 140 == 43
}
pub fn soft_404(i: usize) -> bool {
    i % 150 == 47
}
pub fn nofollow_page(i: usize) -> bool {
    i % 160 == 53
}
/// Feuille de style sous `/assets/private/`, que le robots.txt du site interdit (à Googlebot aussi).
pub fn blocked_css(i: usize) -> bool {
    i % 170 == 59
}

/// Pages au long texte commun (seul le numéro change) : quasi-doublons les unes des autres.
/// Bornées à 20 pages pour rester sous le plafond de seau de la détection (200).
pub fn near_duplicate(i: usize) -> bool {
    i % 100 == 29 && i < 2000
}

/// Densité des pages : le corpus « léger » mesure le moteur, le « dense » ressemble à une page de
/// presse (centaines de liens, 250 Ko de HTML) et sert à mesurer la mémoire et le coût d'analyse.
#[derive(Clone, Copy, Debug)]
pub struct Corpus {
    /// Liens supplémentaires par page (0 = les 4 liens du corpus léger).
    pub extra_links: usize,
    /// Texte de remplissage, en Ko.
    pub filler_kb: usize,
}

impl Corpus {
    pub const LIGHT: Corpus = Corpus {
        extra_links: 0,
        filler_kb: 0,
    };
    pub const DENSE: Corpus = Corpus {
        extra_links: 300,
        filler_kb: 250,
    };
}

const LOREM: &str = "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat. Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla pariatur. Excepteur sint occaecat cupidatat non proident, sunt in culpa qui officia deserunt mollit anim id est laborum. ";

/// La page `i` renvoie vers ces pages (au moins `i+1`, donc tout le site est atteignable depuis `/p/0`).
fn targets(i: usize, n: usize, extra: usize) -> BTreeSet<usize> {
    [(1, 1), (7, 3), (13, 5), (31, 11)]
        .iter()
        .map(|(a, b)| (i * a + b) % n)
        .chain((0..extra).map(|j| (i + 2 + j * 7) % n))
        .filter(|&t| t != i)
        .collect()
}

#[cfg(test)]
pub fn page_html(i: usize, n: usize) -> String {
    page_html_with(i, n, Corpus::LIGHT)
}

pub fn page_html_with(i: usize, n: usize, corpus: Corpus) -> String {
    let amp = if amp_without_canonical(i) { " amp" } else { "" };
    let mut h = format!("<!doctype html><html{amp} lang=\"fr\"><head><meta charset=\"utf-8\">");
    if !missing_title(i) {
        let title = if duplicate_title(i) {
            "Titre dupliqué".to_owned()
        } else if soft_404(i) {
            format!("Page introuvable {i}")
        } else {
            format!("Page {i}")
        };
        let _ = write!(h, "<title>{title}</title>");
        if doubled_title(i) {
            h.push_str("<title>Second titre</title>");
        }
    }
    if !missing_description(i) {
        let _ = write!(
            h,
            "<meta name=\"description\" content=\"Description de la page {i}\">"
        );
    }
    // Une seule meta robots : seule la première est lue.
    let robots: Vec<&str> = [(no_snippet(i), "nosnippet"), (nofollow_page(i), "nofollow")]
        .into_iter()
        .filter_map(|(on, token)| on.then_some(token))
        .collect();
    if !robots.is_empty() {
        let _ = write!(
            h,
            "<meta name=\"robots\" content=\"{}\">",
            robots.join(", ")
        );
    }
    if blocked_css(i) {
        let _ = write!(
            h,
            "<link rel=\"stylesheet\" href=\"/assets/private/{i}.css\">"
        );
    }
    h.push_str("</head><body>");
    if !missing_h1(i) {
        let _ = write!(h, "<h1>Page {i}</h1>");
    }
    h.push_str(
        "<p>Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor.</p>",
    );
    for _ in 0..(corpus.filler_kb * 1024 / LOREM.len()) {
        let _ = write!(h, "<p>{LOREM}</p>");
    }
    // Pas dans le corpus dense : son remplissage identique le rend déjà quasi-dupliqué.
    let near = near_duplicate(i) && corpus.filler_kb == 0;
    if near {
        for _ in 0..8 {
            let _ = write!(h, "<p>{LOREM}</p>");
        }
    }
    if unsized_image(i) {
        let _ = write!(h, "<img src=\"/img/{i}.png\" alt=\"Illustration {i}\">");
    }
    if json_ld_incomplete(i) {
        h.push_str("<script type=\"application/ld+json\">{\"@type\":\"Article\"}</script>");
    }
    if faq_schema(i) {
        h.push_str("<script type=\"application/ld+json\">{\"@type\":\"FAQPage\",\"mainEntity\":[{\"@type\":\"Question\"}]}</script>");
    }
    if untyped_microdata(i) {
        h.push_str("<div itemscope></div>");
    }
    h.push_str("<nav>");
    for t in targets(i, n, corpus.extra_links) {
        // Texte constant sur les pages quasi-dupliquées : seul leur numéro les distingue.
        if near {
            let _ = write!(h, "<a href=\"/p/{t}\">Aller à la page</a>");
        } else {
            let _ = write!(h, "<a href=\"/p/{t}\">Page {t}</a>");
        }
    }
    if empty_anchor(i) {
        h.push_str("<a href=\"/p/0\"></a>");
    }
    if generic_anchor(i) {
        h.push_str("<a href=\"/p/0\">cliquez ici</a>");
    }
    if broken_link(i) {
        let _ = write!(h, "<a href=\"/missing/{i}\">Lien cassé</a>");
    }
    h.push_str("</nav></body></html>");
    h
}

/// Nombre de problèmes que chaque règle doit trouver sur un site de `n` pages (vérité terrain).
/// `dense` : le corpus dense (remplissage identique sur toutes les pages) ne vérifie pas les quasi-doublons.
pub fn expected(n: usize, dense: bool) -> BTreeMap<&'static str, usize> {
    let count = |f: fn(usize) -> bool| (0..n).filter(|&i| f(i)).count();
    let dup = count(duplicate_title);
    let broken = count(broken_link);
    let near = count(near_duplicate);
    let mut exp = BTreeMap::from([
        ("anchor-empty", count(empty_anchor)),
        ("anchor-generic", count(generic_anchor)),
        ("json-ld-missing-property", count(json_ld_incomplete)),
        // L'Article incomplet n'a ni auteur ni date.
        ("article-author-missing", count(json_ld_incomplete)),
        ("article-date-missing", count(json_ld_incomplete)),
        ("ai-snippet-blocked", count(no_snippet)),
        ("schema-faq-howto-retired", count(faq_schema)),
        ("microdata-no-type", count(untyped_microdata)),
        ("amp-no-canonical", count(amp_without_canonical)),
        ("title-missing", count(missing_title)),
        ("title-duplicate", if dup >= 2 { dup } else { 0 }),
        ("description-missing", count(missing_description)),
        ("h1-missing", count(missing_h1)),
        ("broken-link", broken),
        ("http-error", broken),
        ("head-tags-duplicate", count(doubled_title)),
        ("image-size-missing", count(unsized_image)),
        ("nofollow-page", count(nofollow_page)),
        ("resource-blocked", count(blocked_css)),
        // Le départ `/p/0` ne déclare pas d'icône.
        ("favicon-missing", usize::from(n > 0)),
    ]);
    if !dense {
        exp.insert("content-near-duplicate", if near >= 2 { near } else { 0 });
        // Le corpus dense dépasse le plafond de mots : un title « introuvable » y passe pour un article.
        // La page n° 404 s'intitule « Page 404 » : courte, elle passe aussi pour une fausse 404
        // (comportement voulu de l'heuristique, pas un faux compte).
        exp.insert("soft-404", count(soft_404) + usize::from(n > 404));
    } else {
        exp.insert("soft-404", 0);
    }
    exp
}

fn route(path: &str, n: usize, corpus: Corpus) -> (u16, &'static str, String) {
    if path == "/robots.txt" {
        return (
            200,
            "text/plain",
            "User-agent: *\nAllow: /\nDisallow: /assets/private/\n".into(),
        );
    }
    if let Some(i) = path
        .strip_prefix("/p/")
        .and_then(|s| s.parse::<usize>().ok())
        .filter(|&i| i < n)
    {
        return (
            200,
            "text/html; charset=utf-8",
            page_html_with(i, n, corpus),
        );
    }
    (
        404,
        "text/html; charset=utf-8",
        "<html><body>introuvable</body></html>".into(),
    )
}

fn find(buf: &[u8], needle: &[u8]) -> Option<usize> {
    buf.windows(needle.len()).position(|w| w == needle)
}

async fn handle(mut s: TcpStream, n: usize, delay: Duration, corpus: Corpus) {
    let mut buf = Vec::with_capacity(2048);
    loop {
        let end = loop {
            if let Some(p) = find(&buf, b"\r\n\r\n") {
                break p + 4;
            }
            let mut tmp = [0u8; 2048];
            match s.read(&mut tmp).await {
                Ok(0) | Err(_) => return,
                Ok(k) => buf.extend_from_slice(&tmp[..k]),
            }
            if buf.len() > 16 * 1024 {
                return;
            }
        };
        let head = String::from_utf8_lossy(&buf[..end]).into_owned();
        buf.drain(..end);
        let (status, ctype, body) = route(head.split_whitespace().nth(1).unwrap_or("/"), n, corpus);
        if !delay.is_zero() {
            tokio::time::sleep(delay).await; // latence réseau simulée
        }
        let resp = format!(
            "HTTP/1.1 {status} X\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: keep-alive\r\n\r\n{body}",
            body.len()
        );
        if s.write_all(resp.as_bytes()).await.is_err() {
            return;
        }
    }
}

/// Sert le site (connexions persistantes, comme un vrai serveur). Ne rend jamais la main.
pub async fn serve(listener: TcpListener, n: usize, delay: Duration, corpus: Corpus) {
    loop {
        if let Ok((sock, _)) = listener.accept().await {
            tokio::spawn(handle(sock, n, delay, corpus));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use callistseo_core::CrawlConfig;
    use std::collections::HashMap;

    #[test]
    fn generator_matches_its_own_ground_truth() {
        let n = 500;
        let pages: Vec<String> = (0..n).map(|i| page_html(i, n)).collect();
        let without = |marker: &str| pages.iter().filter(|p| !p.contains(marker)).count();
        let exp = expected(n, false);
        assert_eq!(without("<title>"), exp["title-missing"]);
        assert_eq!(without("name=\"description\""), exp["description-missing"]);
        assert_eq!(without("<h1>"), exp["h1-missing"]);
        assert_eq!(
            pages.iter().filter(|p| p.contains("/missing/")).count(),
            exp["broken-link"]
        );
        assert_eq!(
            pages
                .iter()
                .filter(|p| p.contains("Titre dupliqué"))
                .count(),
            exp["title-duplicate"]
        );
        assert!(pages[0] != pages[1] && page_html(3, n) == page_html(3, n)); // déterministe
        assert_eq!(expected(10, false)["title-duplicate"], 1 - 1); // un titre « dupliqué » seul n'est pas un doublon
    }

    #[test]
    fn dense_pages_look_like_press_pages_and_keep_the_same_defects() {
        let html = page_html_with(5, 1_000, Corpus::DENSE);
        assert!(html.len() > 250_000, "taille {}", html.len());
        assert!(html.matches("<a href").count() >= 300);
        // Mêmes défauts injectés que le corpus léger (h1 absent sur i % 30 == 5), donc mêmes comptes attendus.
        assert!(!html.contains("<h1>"));
        assert!(page_html_with(6, 1_000, Corpus::DENSE).contains("<h1>"));
    }

    #[test]
    fn every_page_is_reachable_from_the_start() {
        let n = 300;
        let mut seen = BTreeSet::from([0usize]);
        let mut todo = vec![0usize];
        while let Some(i) = todo.pop() {
            for t in targets(i, n, 0) {
                if seen.insert(t) {
                    todo.push(t);
                }
            }
        }
        assert_eq!(seen.len(), n);
    }

    /// Exactitude des règles : le crawler et `seo-rules` retrouvent exactement les défauts injectés.
    #[tokio::test]
    async fn crawler_and_rules_find_exactly_the_injected_defects() {
        let n = 400;
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(serve(listener, n, Duration::ZERO, Corpus::LIGHT));

        let cfg = CrawlConfig {
            allow_private: true,
            max_pages: n + n / 40 + 10,
            max_depth: 100,
            ..CrawlConfig::default()
        };
        let start = url::Url::parse(&format!("http://127.0.0.1:{port}/p/0")).unwrap();
        let pages = callistseo_crawler::crawl(start, cfg).await.unwrap();
        server.abort();

        let html = pages.iter().filter(|p| p.is_html).count();
        assert_eq!(html, n, "toutes les pages du site sont explorées");
        let mut found: HashMap<&str, usize> = HashMap::new();
        for i in
            callistseo_seo_rules::analyze(&pages, &callistseo_seo_rules::RulesConfig::default())
        {
            *found.entry(i.rule).or_default() += 1;
        }
        for (rule, want) in expected(n, false) {
            assert_eq!(found.get(rule).copied().unwrap_or(0), want, "règle {rule}");
        }
    }
}
