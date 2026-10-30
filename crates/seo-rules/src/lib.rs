//! Règles SEO : fonctions pures sur les pages crawlées (ni réseau, ni base de données).
//! Chaque règle constate un fait ; les seuils éditoriaux sont dans `RulesConfig`.

mod external;
mod extra;
mod info;

pub use external::{external_issues, Measured};
pub use info::{rule_info, RuleInfo, RULES};

use callistseo_core::{CrawledPage, Issue, ROBOTS_BLOCKED};
use std::collections::{HashMap, HashSet};
use url::Url;

#[derive(Debug, Clone)]
pub struct RulesConfig {
    /// Au-delà, une page est jugée profonde (règle éditoriale).
    pub max_depth: usize,
    /// Nombre de variantes `?param` d'un même chemin au-delà duquel on alerte.
    pub max_query_variants: usize,
    /// Longueurs conseillées, en caractères (title et meta description).
    pub title_min: usize,
    pub title_max: usize,
    pub description_min: usize,
    pub description_max: usize,
    /// Sous ce nombre de mots, une page est jugée peu fournie.
    pub min_words: usize,
    /// Délai (ms) jusqu'aux premiers octets au-delà duquel une réponse est jugée lente.
    pub slow_ms: u64,
    /// Longueur d'URL (caractères) au-delà de laquelle on la juge trop longue.
    pub url_max: usize,
}

impl Default for RulesConfig {
    fn default() -> Self {
        Self {
            max_depth: 5,
            max_query_variants: 20,
            title_min: 30,
            title_max: 60,
            description_min: 70,
            description_max: 160,
            min_words: 200,
            slow_ms: 1000,
            url_max: 115,
        }
    }
}

fn is_noindex(p: &CrawledPage) -> bool {
    p.meta
        .robots
        .as_deref()
        .is_some_and(|r| r.contains("noindex"))
        || p.facts
            .header("x-robots-tag")
            .is_some_and(|h| h.to_lowercase().contains("noindex"))
}

/// Page dont le canonical désigne une autre URL : elle déclare elle-même n'être qu'une variante,
/// donc elle n'est pas un doublon (c'est la réponse voulue au contenu dupliqué).
pub(crate) fn canonical_elsewhere(p: &CrawledPage) -> bool {
    p.meta.canonical.as_ref().is_some_and(|c| *c != p.url)
}

/// Page qu'on veut voir indexée : HTML 2xx, sans noindex (meta ou en-tête), et non canonicalisée
/// vers une autre URL. C'est la liste d'un sitemap.
pub fn is_indexable(p: &CrawledPage) -> bool {
    p.is_html
        && p.status.is_some_and(|s| (200..300).contains(&s))
        && !is_noindex(p)
        && !canonical_elsewhere(p)
}

pub fn analyze(pages: &[CrawledPage], cfg: &RulesConfig) -> Vec<Issue> {
    let by_url: HashMap<&Url, &CrawledPage> = pages.iter().map(|p| (&p.url, p)).collect();
    let mut out = Vec::new();
    let mut add = |rule: &'static str, url: &Url, message: String| {
        let severity = rule_info(rule)
            .unwrap_or_else(|| panic!("règle sans entrée dans la table : {rule}"))
            .severity;
        out.push(Issue {
            rule,
            severity,
            url: url.clone(),
            message,
        });
    };

    for p in pages {
        let blocked = p.error.as_deref() == Some(ROBOTS_BLOCKED);
        // HTTP
        if let Some(e) = p.error.as_ref().filter(|_| !blocked) {
            add("network-error", &p.url, e.clone());
        }
        match p.status {
            Some(s) if s >= 400 => add("http-error", &p.url, format!("statut HTTP {s}")),
            Some(s) if (300..400).contains(&s) => match redirect_hops(p, &by_url) {
                Hops::Loop if p.redirect_to.as_ref() == Some(&p.url) => add(
                    "redirect-loop",
                    &p.url,
                    "redirige vers elle-même : souvent un cookie exigé par le serveur (un navigateur le garde, pas le robot)".into(),
                ),
                Hops::Loop => add("redirect-loop", &p.url, "boucle de redirections".into()),
                Hops::Chain(n) if n >= 2 => add(
                    "redirect-chain",
                    &p.url,
                    format!("chaîne de {n} redirections"),
                ),
                // Une 302/303/307 annonce un retour de l'ancienne URL : rarement ce qu'on veut.
                _ if matches!(s, 302 | 303 | 307) => add(
                    "redirect-temporary",
                    &p.url,
                    format!("redirection temporaire {s}"),
                ),
                _ => add("redirect", &p.url, format!("redirection {s}")),
            },
            _ => {}
        }
        if p.depth > cfg.max_depth {
            add(
                "deep-page",
                &p.url,
                format!("profondeur {} (> {})", p.depth, cfg.max_depth),
            );
        }
        if !p.is_html {
            continue;
        }

        // Métadonnées et titres
        let m = &p.meta;
        match m.title.as_deref() {
            None => add("title-missing", &p.url, "balise <title> absente".into()),
            Some("") => add("title-empty", &p.url, "balise <title> vide".into()),
            _ => {}
        }
        match m.description.as_deref() {
            None => add(
                "description-missing",
                &p.url,
                "meta description absente".into(),
            ),
            Some("") => add("description-empty", &p.url, "meta description vide".into()),
            _ => {}
        }
        match m.h1.len() {
            0 => add("h1-missing", &p.url, "aucun H1".into()),
            1 => {}
            n => add("h1-multiple", &p.url, format!("{n} H1")),
        }

        // Indexation
        let noindex = is_noindex(p);
        // L'en-tête X-Robots-Tag a sa propre règle (`noindex-header`, dans `extra`).
        if p.meta
            .robots
            .as_deref()
            .is_some_and(|r| r.contains("noindex"))
        {
            add("noindex", &p.url, "meta robots noindex".into());
        }
        if let Some(c) = m.canonical.as_ref().filter(|c| **c != p.url) {
            if noindex {
                add(
                    "noindex-canonical",
                    &p.url,
                    format!("noindex et canonical vers {c}"),
                );
            }
            if let Some(t) = by_url.get(c) {
                if t.status.is_some_and(|s| s >= 300) || (t.error.is_some() && t.status.is_none()) {
                    add(
                        "canonical-target",
                        &p.url,
                        format!("canonical vers une URL redirigée ou en erreur : {c}"),
                    );
                }
            }
        }

        // Liens internes cassés, ou qui passent par une redirection
        let mut redirected: Vec<&Url> = Vec::new();
        for l in &p.links {
            if let Some(t) = by_url.get(&**l) {
                // Une cible sans réponse (délai dépassé, réseau) n'est pas « cassée » : `network-error`
                // la signale déjà une fois sur la page elle-même, au lieu d'une fois par lien qui y mène.
                if t.status.is_some_and(|s| s >= 400) {
                    add("broken-link", &p.url, format!("lien cassé vers {l}"));
                } else if t.status.is_some_and(|s| (300..400).contains(&s))
                    && !redirected.contains(&&**l)
                {
                    redirected.push(l);
                }
            }
        }
        // Un seul constat par page : un menu qui mène à une redirection ne doit pas noyer le rapport.
        if let Some(first) = redirected.first() {
            add(
                "link-to-redirect",
                &p.url,
                match redirected.len() {
                    1 => format!("lien vers une redirection : {first}"),
                    n => format!("{n} liens vers des redirections, dont {first}"),
                },
            );
        }
    }

    // Sitemaps
    if pages.iter().any(|p| p.in_sitemap) {
        let inbound: HashSet<&Url> = pages
            .iter()
            .flat_map(|p| {
                p.links
                    .iter()
                    .map(|l| &**l)
                    .chain(p.redirect_to.iter())
                    .filter(move |u| **u != p.url)
            })
            .collect();
        for p in pages {
            if p.in_sitemap {
                let unreachable = p.status.is_some_and(|s| s >= 400)
                    || (p.status.is_none()
                        && p.error.as_deref().is_some_and(|e| e != ROBOTS_BLOCKED));
                if unreachable {
                    add(
                        "sitemap-url-error",
                        &p.url,
                        "URL du sitemap inaccessible".into(),
                    );
                } else if p.status.is_some_and(|s| (300..400).contains(&s)) {
                    add(
                        "sitemap-url-redirect",
                        &p.url,
                        "URL du sitemap redirigée".into(),
                    );
                } else if p.error.as_deref() == Some(ROBOTS_BLOCKED) {
                    add(
                        "sitemap-url-blocked",
                        &p.url,
                        "URL du sitemap interdite par robots.txt".into(),
                    );
                } else if is_noindex(p) {
                    add(
                        "sitemap-url-noindex",
                        &p.url,
                        "URL du sitemap en noindex".into(),
                    );
                }
                if p.depth > 0 && !inbound.contains(&p.url) {
                    add(
                        "orphan-page",
                        &p.url,
                        "listée au sitemap mais sans lien entrant".into(),
                    );
                }
            } else if p.is_html
                && p.status.is_some_and(|s| (200..300).contains(&s))
                && !is_noindex(p)
            {
                add("not-in-sitemap", &p.url, "absente du sitemap".into());
            }
        }
    }

    // Doublons (pages HTML 2xx indexables uniquement)
    type Key = fn(&CrawledPage) -> Option<String>;
    let keys: [(&'static str, &str, Key); 3] = [
        ("title-duplicate", "title", |p| p.meta.title.clone()),
        ("description-duplicate", "meta description", |p| {
            p.meta.description.clone()
        }),
        ("h1-duplicate", "H1", |p| p.meta.h1.first().cloned()),
    ];
    for (rule, label, key) in keys {
        let mut groups: HashMap<String, Vec<&CrawledPage>> = HashMap::new();
        let indexable = pages.iter().filter(|p| {
            p.is_html
                && p.status.is_some_and(|s| (200..300).contains(&s))
                && !is_noindex(p)
                && !canonical_elsewhere(p)
        });
        for p in indexable {
            if let Some(v) = key(p).filter(|v| !v.is_empty()) {
                groups.entry(v).or_default().push(p);
            }
        }
        for (v, ps) in groups.iter().filter(|(_, ps)| ps.len() > 1) {
            for p in ps {
                add(
                    rule,
                    &p.url,
                    format!("{label} partagé par {} pages : « {v} »", ps.len()),
                );
            }
        }
    }

    // Explosion de paramètres : un même chemin décliné en beaucoup de requêtes
    let mut variants: HashMap<(&str, &str), Vec<&Url>> = HashMap::new();
    for p in pages.iter().filter(|p| p.url.query().is_some()) {
        variants
            .entry((p.url.host_str().unwrap_or(""), p.url.path()))
            .or_default()
            .push(&p.url);
    }
    for ((_, path), urls) in variants
        .iter()
        .filter(|(_, u)| u.len() > cfg.max_query_variants)
    {
        add(
            "query-explosion",
            urls[0],
            format!("{} variantes de paramètres sur {path}", urls.len()),
        );
    }

    extra::run(pages, cfg, &mut add);

    out.sort_by(|a, b| (a.severity, a.rule, &a.url).cmp(&(b.severity, b.rule, &b.url)));
    out
}

enum Hops {
    Chain(usize),
    Loop,
}

/// Suit les redirections connues depuis `p` ; s'arrête sur une cible non crawlée.
fn redirect_hops(p: &CrawledPage, by_url: &HashMap<&Url, &CrawledPage>) -> Hops {
    let mut seen: HashSet<&Url> = HashSet::from([&p.url]);
    let (mut cur, mut n) = (p, 0);
    while let Some(next) = cur.redirect_to.as_ref() {
        n += 1;
        if !seen.insert(next) {
            return Hops::Loop;
        }
        match by_url.get(next) {
            Some(t) => cur = t,
            None => break,
        }
    }
    Hops::Chain(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use callistseo_core::PageMeta;
    use std::sync::Arc;

    fn u(p: &str) -> Url {
        Url::parse(&format!("http://a.test{p}")).unwrap()
    }

    fn page(path: &str, status: u16) -> CrawledPage {
        CrawledPage {
            facts: Default::default(),
            in_sitemap: false,
            is_html: status < 300,
            meta: PageMeta {
                title: Some(format!("Titre de la page {path} pour les tests")),
                description: Some(format!(
                    "Description de la page {path} écrite assez longuement pour dépasser le seuil minimal"
                )),
                h1: vec![format!("h {path}")],
                ..PageMeta::default()
            },
            url: u(path),
            depth: 0,
            status: Some(status),
            redirect_to: None,
            links: vec![],
            error: None,
        }
    }

    /// Mesure (à lancer à la main) : `cargo test -p callistseo-seo-rules --release perf_analyze -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn perf_analyze() {
        let pages: Vec<CrawledPage> = (0..50_000)
            .map(|i| {
                let mut p = page(&format!("/p{i}"), 200);
                p.in_sitemap = i % 2 == 0;
                p.meta.title = Some(format!("Titre {}", i % 40_000));
                p.facts.content_hash = Some((i % 45_000).to_string());
                p.facts.json_ld = vec![r#"{"@context":"https://schema.org","@type":"Article","headline":"x","datePublished":"2026-01-01"}"#.into(); 3];
                p.links = (0..20).map(|k| Arc::new(u(&format!("/p{}", (i * 7 + k) % 50_000)))).collect();
                p
            })
            .collect();
        let t = std::time::Instant::now();
        let n = analyze(&pages, &RulesConfig::default()).len();
        eprintln!(
            "analyze : {} pages, {n} issues en {:?}",
            pages.len(),
            t.elapsed()
        );
    }

    fn rules(pages: &[CrawledPage]) -> Vec<(&'static str, String)> {
        analyze(pages, &RulesConfig::default())
            .into_iter()
            // Les pages de test sont en HTTP, sans liens ni canonical : ces règles ont leur propre test.
            .filter(|i| {
                !matches!(
                    i.rule,
                    "insecure-http" | "dead-end-page" | "canonical-missing"
                )
            })
            .map(|i| (i.rule, i.url.path().to_owned()))
            .collect()
    }

    #[test]
    fn clean_site_has_no_issues() {
        let mut home = page("/", 200);
        home.links = vec![Arc::new(u("/a"))];
        assert!(rules(&[home, page("/a", 200)]).is_empty());
    }

    #[test]
    fn unreachable_targets_are_not_broken_links() {
        let mut home = page("/", 200);
        home.links = vec![Arc::new(u("/slow")), Arc::new(u("/gone"))];
        let mut slow = page("/slow", 200);
        slow.status = None;
        slow.is_html = false;
        slow.error = Some("operation timed out".into());
        let r = rules(&[home, slow, page("/gone", 404)]);
        // Une seule erreur de lien : celle vers la page en 404 ; le délai dépassé reste `network-error`.
        assert_eq!(r.iter().filter(|x| x.0 == "broken-link").count(), 1);
        assert!(r.contains(&("network-error", "/slow".into())));
    }

    #[test]
    fn self_redirect_is_explained() {
        let mut a = page("/a", 301);
        a.redirect_to = Some(u("/a"));
        let found: Vec<_> = analyze(&[a], &RulesConfig::default())
            .into_iter()
            .filter(|i| i.rule == "redirect-loop")
            .collect();
        assert_eq!(found.len(), 1);
        assert!(found[0].message.contains("cookie"), "{}", found[0].message);
    }

    #[test]
    fn http_and_broken_links() {
        let mut home = page("/", 200);
        home.links = vec![Arc::new(u("/gone")), Arc::new(u("/blocked"))];
        let mut blocked = page("/blocked", 200);
        blocked.status = None;
        blocked.is_html = false;
        blocked.error = Some(ROBOTS_BLOCKED.into());
        let r = rules(&[home, page("/gone", 404), blocked]);
        assert!(r.contains(&("http-error", "/gone".into())));
        assert!(r.contains(&("broken-link", "/".into())));
        assert_eq!(
            r.len(),
            2,
            "une URL bloquée par robots.txt n'est pas une panne : {r:?}"
        );
    }

    #[test]
    fn severity_comes_from_the_rules_table() {
        let mut no_title = page("/", 200);
        no_title.meta.title = None;
        let mut gone = page("/gone", 404);
        gone.is_html = false;
        for issue in analyze(&[no_title, gone], &RulesConfig::default()) {
            assert_eq!(
                issue.severity,
                rule_info(issue.rule).unwrap().severity,
                "{}",
                issue.rule
            );
        }
        let by_rule = |rule| {
            analyze(&[page("/x", 404)], &RulesConfig::default())
                .into_iter()
                .find(|i| i.rule == rule)
                .map(|i| i.severity)
        };
        assert_eq!(
            by_rule("http-error"),
            Some(callistseo_core::Severity::Error)
        );
    }

    #[test]
    fn meta_rules() {
        let mut p = page("/", 200);
        p.meta = PageMeta {
            title: Some(String::new()),
            h1: vec!["a".into(), "b".into()],
            ..PageMeta::default()
        };
        let r: Vec<_> = rules(&[p]).into_iter().map(|x| x.0).collect();
        // Triés par gravité puis par nom : deux avertissements, puis l'info.
        assert_eq!(r, ["description-missing", "title-empty", "h1-multiple"]);
    }

    #[test]
    fn duplicates_ignore_noindex() {
        let (mut a, mut b, mut c) = (page("/a", 200), page("/b", 200), page("/c", 200));
        for p in [&mut a, &mut b, &mut c] {
            p.meta.title = Some("same".into());
        }
        c.meta.robots = Some("noindex".into());
        let r = rules(&[a, b, c]);
        assert_eq!(r.iter().filter(|x| x.0 == "title-duplicate").count(), 2);
        assert!(r.contains(&("noindex", "/c".into())));
    }

    #[test]
    fn redirect_chain_and_loop() {
        let (mut a, mut b, c) = (page("/a", 301), page("/b", 301), page("/c", 200));
        a.redirect_to = Some(u("/b"));
        b.redirect_to = Some(u("/c"));
        let r = rules(&[a.clone(), b.clone(), c]);
        assert!(
            r.contains(&("redirect-chain", "/a".into())) && r.contains(&("redirect", "/b".into()))
        );

        b.redirect_to = Some(u("/a"));
        let r = rules(&[a, b]);
        assert_eq!(r.iter().filter(|x| x.0 == "redirect-loop").count(), 2);
    }

    #[test]
    fn canonical_to_redirect_and_noindex_conflict() {
        let mut a = page("/a", 200);
        a.meta.canonical = Some(u("/old"));
        a.meta.robots = Some("noindex".into());
        let mut old = page("/old", 301);
        old.redirect_to = Some(u("/a"));
        let r: Vec<_> = rules(&[a, old]).into_iter().map(|x| x.0).collect();
        assert!(r.contains(&"canonical-target") && r.contains(&"noindex-canonical"));
    }

    #[test]
    fn sitemap_rules() {
        let mut home = page("/", 200);
        home.links = vec![Arc::new(u("/linked"))];
        let mut linked = page("/linked", 200);
        linked.in_sitemap = true;
        let mut orphan = page("/orphan", 200);
        (orphan.in_sitemap, orphan.depth) = (true, 1);
        let mut gone = page("/gone", 404);
        (gone.in_sitemap, gone.depth) = (true, 1);
        let r = rules(&[home, linked, orphan, gone, page("/extra", 200)]);
        assert!(r.contains(&("sitemap-url-error", "/gone".into())));
        assert!(r.contains(&("orphan-page", "/orphan".into())));
        assert!(!r.contains(&("orphan-page", "/linked".into())));
        assert!(r.contains(&("not-in-sitemap", "/extra".into())));
    }

    #[test]
    fn deep_pages_and_query_explosion() {
        let mut deep = page("/d", 200);
        deep.depth = 9;
        let mut pages = vec![deep];
        for i in 0..25 {
            let mut p = page("/list", 200);
            p.url = Url::parse(&format!("http://a.test/list?p={i}")).unwrap();
            p.meta.title = Some(format!("t{i}"));
            p.meta.description = Some(format!("d{i}"));
            pages.push(p);
        }
        let r = rules(&pages);
        assert!(r.contains(&("deep-page", "/d".into())));
        assert_eq!(r.iter().filter(|x| x.0 == "query-explosion").count(), 1);
    }

    // ---- règles sur les faits mesurés -----------------------------------------------------------

    use callistseo_core::PageFacts;

    /// Page HTML « mesurée » et saine : les faits d'un audit récent, sans défaut.
    fn measured(path: &str) -> CrawledPage {
        let mut p = page(path, 200);
        p.url = Url::parse(&format!("https://a.test{path}")).unwrap();
        p.facts = PageFacts {
            ttfb_ms: Some(80),
            headers: vec![
                ("strict-transport-security".into(), "max-age=1".into()),
                ("x-content-type-options".into(), "nosniff".into()),
            ],
            content_hash: Some(format!("h{path}")),
            word_count: Some(500),
            lang: Some("fr".into()),
            viewport: Some("width=device-width".into()),
            h2: vec!["Sous-titre".into()],
            ..PageFacts::default()
        };
        p
    }

    fn only(p: CrawledPage) -> Vec<&'static str> {
        rules(&[p]).into_iter().map(|(r, _)| r).collect()
    }

    #[test]
    fn a_healthy_measured_page_has_no_issue_and_missing_facts_never_fire() {
        assert!(only(measured("/")).is_empty());
        // Audit d'avant la v3 : aucun fait, donc aucune de ces règles.
        let mut old = page("/", 200);
        old.url = Url::parse("https://a.test/").unwrap();
        assert!(only(old).is_empty());
    }

    #[test]
    fn length_rules() {
        let mut p = measured("/");
        p.meta.title = Some("x".repeat(61));
        p.meta.description = Some("y".repeat(20));
        assert_eq!(only(p), ["description-too-short", "title-too-long"]);
        let mut p = measured("/");
        p.meta.title = Some("court".into());
        p.meta.description = Some("y".repeat(161));
        assert_eq!(only(p), ["description-too-long", "title-too-short"]);
        let mut p = measured("/");
        p.meta.title = Some("x".repeat(60)); // pile à la limite : accepté
        assert!(only(p).is_empty());
    }

    #[test]
    fn x_robots_tag_noindex_counts_as_noindex_everywhere() {
        let mut p = measured("/");
        p.facts
            .headers
            .push(("x-robots-tag".into(), "NoIndex, nofollow".into()));
        // « nofollow » dans le même en-tête a sa propre règle.
        assert_eq!(only(p.clone()), ["nofollow-page", "noindex-header"]);
        // Il retire la page des doublons de title : deux pages au même title, l'une noindex par en-tête.
        let mut q = measured("/b");
        q.meta.title = p.meta.title.clone();
        assert!(!rules(&[p, q]).iter().any(|(r, _)| *r == "title-duplicate"));
    }

    #[test]
    fn canonicalized_pages_are_not_duplicates() {
        let count =
            |pages: &[CrawledPage], rule: &str| rules(pages).iter().filter(|x| x.0 == rule).count();
        let mut all = [measured("/a"), measured("/b"), measured("/c")];
        for p in &mut all {
            p.meta.title = Some("same".into());
            p.meta.description =
                Some("same description assez longue pour passer le seuil minimal".into());
            p.facts.content_hash = Some("same".into());
        }
        // /b et /c (paramètre de suivi, pagination…) déclarent /a comme version de référence.
        all[1].meta.canonical = Some(u("/a"));
        all[2].meta.canonical = Some(u("/a"));
        for rule in [
            "title-duplicate",
            "description-duplicate",
            "content-duplicate",
            "content-near-duplicate",
        ] {
            assert_eq!(count(&all, rule), 0, "{rule}");
        }
        // Sans canonical, les mêmes pages sont bien des doublons.
        all[1].meta.canonical = None;
        all[2].meta.canonical = None;
        assert_eq!(count(&all, "title-duplicate"), 3);
        assert_eq!(count(&all, "content-duplicate"), 3);
    }

    #[test]
    fn exact_content_duplicates_ignore_short_pages() {
        let (mut a, mut b, mut c) = (measured("/a"), measured("/b"), measured("/c"));
        for p in [&mut a, &mut b, &mut c] {
            p.facts.content_hash = Some("same".into());
        }
        c.facts.word_count = Some(10); // trop court pour être un doublon significatif
        let found: Vec<_> = rules(&[a, b, c])
            .into_iter()
            .filter(|(r, _)| *r == "content-duplicate")
            .map(|(_, p)| p)
            .collect();
        assert_eq!(found, ["/a", "/b"]);
    }

    #[test]
    fn hreflang_rules() {
        // x-default a son propre test (`new_rules_on_existing_facts`).
        let rules = |ps: &[CrawledPage]| -> Vec<_> {
            rules(ps)
                .into_iter()
                .filter(|(r, _)| *r != "hreflang-x-default-missing")
                .collect()
        };
        let en = |p: &CrawledPage| ("en".to_owned(), p.url.to_string());
        let (mut fr, mut english) = (measured("/fr"), measured("/en"));
        let (fr_self, en_self) = (("fr".to_owned(), fr.url.to_string()), en(&english));
        fr.facts.hreflang = vec![fr_self.clone(), en_self.clone()];
        english.facts.hreflang = vec![en_self.clone(), fr_self.clone()];
        assert!(
            rules(&[fr.clone(), english.clone()]).is_empty(),
            "paire réciproque correcte"
        );

        // Pas de lien retour.
        english.facts.hreflang = vec![en_self.clone()];
        let r = rules(&[fr.clone(), english.clone()]);
        assert_eq!(r, [("hreflang-missing-return", "/fr".to_owned())]);

        // Pas d'auto-référence + code invalide.
        fr.facts.hreflang = vec![("english".into(), english.url.to_string())];
        let names: Vec<_> = rules(&[fr.clone(), english.clone()])
            .into_iter()
            .map(|(r, _)| r)
            .collect();
        assert!(names.contains(&"hreflang-invalid-code") && names.contains(&"hreflang-no-self"));

        // Cible en erreur.
        let mut gone = measured("/en");
        gone.status = Some(404);
        fr.facts.hreflang = vec![fr_self, ("en".into(), gone.url.to_string())];
        assert!(rules(&[fr, gone])
            .iter()
            .any(|(r, _)| *r == "hreflang-target-error"));
    }

    #[test]
    fn hreflang_codes() {
        for ok in ["fr", "fr-FR", "x-default", "zh-Hant-TW", "es-419", "fil"] {
            assert!(super::extra::valid_hreflang(ok), "{ok}");
        }
        for bad in ["", "english", "f", "fr_FR", "FR", "fr-", "en-US-"] {
            assert!(!super::extra::valid_hreflang(bad), "{bad}");
        }
    }

    #[test]
    fn security_and_static_checks() {
        let mut p = measured("/");
        p.facts.headers.clear();
        p.facts.images = vec![
            ("http://cdn.test/a.png".into(), None),
            ("/b.png".into(), Some(String::new())),
        ];
        p.facts.lang = None;
        p.facts.viewport = None;
        p.facts.h2.clear();
        p.facts.json_ld = vec![
            r#"{"@type":"Article","headline":"t","author":"a","datePublished":"2026-01-01"}"#
                .into(),
            r#"{"@type":"#.into(),
        ];
        p.facts.ttfb_ms = Some(2500);
        let mut got = only(p);
        got.sort();
        assert_eq!(
            got,
            [
                "h2-missing",
                "hsts-missing",
                "image-alt-missing",
                "json-ld-invalid",
                "lang-missing",
                "mixed-content",
                "nosniff-missing",
                "slow-response",
                "viewport-missing"
            ]
        );
        // Une page en HTTP est signalée (HSTS n'a alors pas de sens).
        let mut http = measured("/");
        http.url = u("/");
        let issues = analyze(&[http], &RulesConfig::default());
        assert_eq!(
            issues
                .iter()
                .map(|i| i.rule)
                .filter(|r| !matches!(*r, "canonical-missing" | "dead-end-page"))
                .collect::<Vec<_>>(),
            ["insecure-http"]
        );
    }

    #[test]
    fn thin_content() {
        let mut p = measured("/");
        p.facts.word_count = Some(50);
        assert_eq!(only(p), ["thin-content"]);
    }

    #[test]
    fn near_duplicates_and_json_ld_properties() {
        let mk = |path: &str, hash: &str, sim: u64| {
            let mut p = measured(path);
            p.facts.word_count = Some(500);
            p.facts.content_hash = Some(hash.into());
            p.facts.simhash = Some(format!("{sim:016x}"));
            p
        };
        let base = 0xdead_beef_0123_4567u64;
        let r = rules(&[
            mk("/a", "1", base),
            mk("/b", "2", base ^ 0b101), // 2 bits d'écart : quasi-doublon
            mk("/c", "3", !base),        // très différente
            mk("/d", "4", base),         // même simhash, hash différent : quasi-doublon
        ]);
        let near: Vec<_> = r
            .iter()
            .filter(|x| x.0 == "content-near-duplicate")
            .collect();
        assert_eq!(near.len(), 3, "{near:?}");
        assert!(near.iter().all(|x| !x.1.is_empty()));

        let mut p = measured("/j");
        p.facts.json_ld = vec![
            r#"{"@graph":[{"@type":"Article","headline":"ok"},{"@type":"Product"}]}"#.into(),
            r#"{"@type":"Event","name":"x","startDate":"2026-01-01","location":""}"#.into(),
        ];
        let miss: Vec<_> = rules(&[p])
            .into_iter()
            .filter(|x| x.0 == "json-ld-missing-property")
            .collect();
        assert_eq!(miss.len(), 2, "{miss:?}");
    }

    #[test]
    fn amp_and_microdata_rules() {
        let mut amp = measured("/amp");
        amp.facts.is_amp = true; // sans canonical
        let mut main = measured("/");
        main.facts.amp_html = Some("http://a.test/gone".into());
        main.facts.microdata_untyped = 2;
        let gone = page("/gone", 404);
        let r = rules(&[amp, main, gone]);
        for id in ["amp-no-canonical", "amp-target-error", "microdata-no-type"] {
            assert_eq!(r.iter().filter(|x| x.0 == id).count(), 1, "{id}: {r:?}");
        }
    }

    #[test]
    fn lastmod_validation() {
        use crate::extra::valid_lastmod;
        for ok in [
            "2026",
            "2026-03",
            "2026-03-04",
            "2026-03-04T10:20:30Z",
            "2026-03-04T10:20:30+02:00",
        ] {
            assert!(valid_lastmod(ok), "{ok}");
        }
        for bad in [
            "",
            "04/03/2026",
            "2026-13-01",
            "2026-03-04T10:20",
            "hier",
            "2026-3-4",
        ] {
            assert!(!valid_lastmod(bad), "{bad}");
        }
        let mut p = measured("/x");
        p.facts.sitemap_lastmod = Some("04/03/2026".into());
        assert_eq!(only(p), ["sitemap-lastmod-invalid"]);
    }

    #[test]
    fn anchor_and_external_link_rules() {
        let mut p = measured("/x");
        p.facts.anchor_empty = 2;
        p.facts.anchor_generic = 1;
        p.facts.external_broken = vec![("http://e.test/a".into(), "404".into())];
        let mut got = only(p);
        got.sort();
        assert_eq!(
            got,
            ["anchor-empty", "anchor-generic", "external-link-broken"]
        );
    }

    #[test]
    fn ai_geo_rules() {
        let ids = |p: CrawledPage| {
            let mut v = only(p);
            v.sort();
            v
        };
        let mut p = measured("/a");
        p.meta.robots = Some("max-snippet: 0, index".into());
        p.facts.ai_bots_blocked = vec!["GPTBot".into()];
        assert_eq!(ids(p), ["ai-crawler-blocked", "ai-snippet-blocked"]);

        let mut p = measured("/b");
        p.meta.robots = Some("max-snippet:50".into());
        assert!(ids(p).is_empty());
        let mut p = measured("/c");
        p.facts
            .headers
            .push(("x-robots-tag".into(), "nosnippet".into()));
        assert_eq!(ids(p), ["ai-snippet-blocked"]);

        let mut p = measured("/d");
        p.facts.json_ld = vec![
            r#"{"@graph":[{"@type":"FAQPage","mainEntity":[1]},{"@type":"BlogPosting","headline":"h","author":{"name":"x"},"dateModified":"2026-01-01"}]}"#.into(),
            r#"{"@type":["Article"],"headline":"h","datePublished":"2026-01-01"}"#.into(),
        ];
        assert_eq!(
            ids(p),
            ["article-author-missing", "schema-faq-howto-retired"]
        );
        let mut p = measured("/e");
        p.facts.json_ld = vec![r#"{"@type":"NewsArticle","headline":"h","author":"x"}"#.into()];
        assert_eq!(ids(p), ["article-date-missing"]);
    }

    /// Règles calculées sur les données déjà collectées (lot A) : un cas qui déclenche, un qui ne
    /// déclenche pas.
    #[test]
    fn new_rules_on_existing_facts() {
        const NEW: &[&str] = &[
            "redirect-temporary",
            "link-to-redirect",
            "canonical-missing",
            "dead-end-page",
            "h1-duplicate",
            "url-too-long",
            "url-uppercase",
            "url-underscore",
            "hreflang-x-default-missing",
            "sitemap-url-blocked",
            "html-too-large",
        ];
        let found = |pages: &[CrawledPage]| -> Vec<(&'static str, String)> {
            analyze(pages, &RulesConfig::default())
                .into_iter()
                .filter(|i| NEW.contains(&i.rule))
                .map(|i| (i.rule, i.url.path().to_owned()))
                .collect()
        };
        let linked = |path: &str, to: &[&str]| {
            let mut p = measured(path);
            p.meta.canonical = Some(p.url.clone());
            p.links = to
                .iter()
                .map(|t| Arc::new(Url::parse(&format!("https://a.test{t}")).unwrap()))
                .collect();
            p
        };

        // Site sain : canonicals, liens entre pages, H1 distincts, hreflang avec x-default.
        let (mut a, b) = (linked("/a", &["/b"]), linked("/b", &["/a"]));
        a.facts.hreflang = vec![
            ("fr".into(), a.url.to_string()),
            ("x-default".into(), a.url.to_string()),
        ];
        assert_eq!(found(&[a.clone(), b.clone()]), []);

        // Redirections : 302 = temporaire (pas « redirect »), 301 reste « redirect » ; un seul constat
        // par page même avec deux liens vers des redirections.
        let mut tmp = linked("/tmp", &[]);
        (tmp.status, tmp.is_html, tmp.redirect_to) = (Some(302), false, Some(b.url.clone()));
        let mut moved = linked("/old", &[]);
        (moved.status, moved.is_html, moved.redirect_to) = (Some(301), false, Some(b.url.clone()));
        let src = linked("/src", &["/tmp", "/old", "/tmp", "/b"]);
        let got = found(&[src, tmp, moved, b.clone()]);
        // Tri de `analyze` : gravité (Avertissement avant Info), puis règle.
        assert_eq!(
            got,
            [
                ("redirect-temporary", "/tmp".to_owned()),
                ("link-to-redirect", "/src".to_owned())
            ]
        );

        // Page indexable sans canonical ni lien interne ; H1 partagé.
        let mut alone = measured("/seule");
        alone.links = vec![
            Arc::new(Url::parse("https://autre.test/").unwrap()),
            Arc::new(alone.url.clone()),
        ];
        let mut twin = linked("/jumelle", &["/seule"]);
        twin.meta.h1 = alone.meta.h1.clone();
        let got = found(&[alone, twin]);
        assert_eq!(
            got,
            [
                ("canonical-missing", "/seule".to_owned()),
                ("dead-end-page", "/seule".to_owned()),
                ("h1-duplicate", "/jumelle".to_owned()),
                ("h1-duplicate", "/seule".to_owned()),
            ]
        );

        // Forme de l'URL, hreflang sans x-default, HTML trop lourd.
        let long = format!("/{}", "a".repeat(120));
        let mut odd = linked("/Mon_Article", &["/b"]);
        odd.facts.hreflang = vec![("fr".into(), odd.url.to_string())];
        odd.facts.size = Some(3 * 1024 * 1024);
        let got = found(&[odd, linked(&long, &["/b"]), b.clone()]);
        assert_eq!(
            got,
            [
                ("html-too-large", "/Mon_Article".to_owned()),
                ("hreflang-x-default-missing", "/Mon_Article".to_owned()),
                ("url-too-long", long.clone()),
                ("url-underscore", "/Mon_Article".to_owned()),
                ("url-uppercase", "/Mon_Article".to_owned()),
            ]
        );

        // Sitemap : URL interdite par robots.txt.
        let mut blocked = linked("/prive", &[]);
        (
            blocked.status,
            blocked.is_html,
            blocked.error,
            blocked.in_sitemap,
        ) = (None, false, Some(ROBOTS_BLOCKED.into()), true);
        a.in_sitemap = true;
        assert_eq!(
            found(&[a, b, blocked]),
            [("sitemap-url-blocked", "/prive".to_owned())]
        );
    }

    /// Règles sur les faits du schéma v16 (lot B), et silence sur un audit plus ancien.
    #[test]
    fn head_social_image_icon_and_soft_404_rules() {
        const NEW: &[&str] = &[
            "head-tags-duplicate",
            "open-graph-missing",
            "image-size-missing",
            "nofollow-page",
            "favicon-missing",
            "soft-404",
            "resource-blocked",
        ];
        let found = |p: CrawledPage| -> Vec<&'static str> {
            analyze(&[p], &RulesConfig::default())
                .into_iter()
                .map(|i| i.rule)
                .filter(|r| NEW.contains(r))
                .collect()
        };
        let v16 = |path: &str| {
            let mut p = measured(path);
            p.facts.title_tags = Some(1);
            p.facts.description_tags = Some(1);
            p.facts.canonical_tags = Some(1);
            p.facts.images_unsized = Some(0);
            p.facts.social = vec!["og:title".into(), "og:image".into()];
            p.facts.icon = Some(true);
            p
        };
        assert_eq!(found(v16("/")), Vec::<&str>::new(), "page saine");
        // Audit d'avant la v16 : aucun fait, aucune règle (hors nofollow, lu dans meta robots).
        assert_eq!(found(measured("/")), Vec::<&str>::new());

        let mut p = v16("/");
        p.facts.canonical_tags = Some(2);
        p.facts.images_unsized = Some(3);
        p.facts.social = vec!["og:title".into()];
        p.facts.icon = Some(false);
        p.meta.robots = Some("index, nofollow".into());
        assert_eq!(
            found(p),
            [
                "head-tags-duplicate",
                "favicon-missing",
                "image-size-missing",
                "nofollow-page",
                "open-graph-missing"
            ]
        );
        // CSS / JS interdits à Googlebot (liste déjà filtrée par le crawler).
        let mut blocked = v16("/r");
        blocked.facts.blocked_resources = vec![
            "https://a.test/assets/app.css".into(),
            "https://a.test/assets/app.js".into(),
        ];
        assert_eq!(found(blocked), ["resource-blocked"]);
        // L'icône ne compte que sur l'accueil (profondeur 0).
        let mut deep = v16("/a");
        (deep.depth, deep.facts.icon) = (1, Some(false));
        assert_eq!(found(deep), Vec::<&str>::new());

        // Fausse 404 : titre d'erreur et page courte ; un long article « Peugeot 404 » ne compte pas.
        let mut missing = v16("/x");
        missing.meta.title = Some("Page introuvable – Mon site".into());
        missing.facts.word_count = Some(40);
        assert_eq!(found(missing), ["soft-404"]);
        let mut car = v16("/peugeot");
        car.meta.title = Some("La Peugeot 404, histoire d’une berline".into());
        assert_eq!(found(car.clone()), Vec::<&str>::new());
        car.facts.word_count = Some(120);
        car.meta.h1 = vec!["Erreur 404".into()];
        assert_eq!(found(car), ["soft-404"]);
    }
}
