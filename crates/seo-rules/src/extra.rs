//! Règles qui s'appuient sur les faits mesurés par URL (`PageFacts`) : longueurs, indexation par
//! en-tête, doublons de contenu, hreflang, sécurité, données structurées, contrôles statiques.
//!
//! Garde-fou : un fait absent (`None`, audit d'avant la v3) ne déclenche jamais de règle.

use crate::{canonical_elsewhere, is_noindex, RulesConfig};
use callistseo_core::CrawledPage;
use std::collections::HashMap;
use url::Url;

type Add<'a> = &'a mut dyn FnMut(&'static str, &Url, String);

fn indexable(p: &CrawledPage) -> bool {
    p.is_html
        && p.status.is_some_and(|s| (200..300).contains(&s))
        && !is_noindex(p)
        && !canonical_elsewhere(p)
}

/// `x-default`, ou langue (2-3 lettres) suivie de sous-étiquettes alphanumériques de 2 à 8 signes
/// (`fr`, `fr-FR`, `zh-Hant-TW`, `es-419`).
pub(crate) fn valid_hreflang(code: &str) -> bool {
    if code == "x-default" {
        return true;
    }
    let mut parts = code.split('-');
    let lang = parts.next().unwrap_or_default();
    (2..=3).contains(&lang.len())
        && lang.bytes().all(|b| b.is_ascii_lowercase())
        && parts.all(|t| (2..=8).contains(&t.len()) && t.bytes().all(|b| b.is_ascii_alphanumeric()))
}

/// Propriétés exigées par Google pour l'éligibilité aux résultats enrichis, par type schema.org.
/// ponytail: sous-ensemble des types courants ; compléter la table au besoin.
const REQUIRED: &[(&str, &[&str])] = &[
    ("Article", &["headline"]),
    ("NewsArticle", &["headline"]),
    ("BlogPosting", &["headline"]),
    ("Product", &["name"]),
    ("Organization", &["name"]),
    ("LocalBusiness", &["name", "address"]),
    ("Event", &["name", "startDate", "location"]),
    ("Recipe", &["name", "image"]),
    ("VideoObject", &["name", "thumbnailUrl", "uploadDate"]),
    (
        "JobPosting",
        &["title", "description", "datePosted", "hiringOrganization"],
    ),
    ("FAQPage", &["mainEntity"]),
    ("BreadcrumbList", &["itemListElement"]),
    ("WebSite", &["url"]),
];

/// Nœuds JSON-LD (objets) d'un bloc : racine, tableau ou `@graph`.
fn json_ld_nodes<'a>(v: &'a serde_json::Value, out: &mut Vec<&'a serde_json::Value>) {
    match v {
        serde_json::Value::Array(a) => a.iter().for_each(|x| json_ld_nodes(x, out)),
        serde_json::Value::Object(o) => {
            if let Some(g) = o.get("@graph") {
                json_ld_nodes(g, out);
            }
            out.push(v);
        }
        _ => {}
    }
}

/// (type, propriétés manquantes) d'un nœud, pour le premier de ses types connus.
fn missing_properties(node: &serde_json::Value) -> Option<(&'static str, Vec<&'static str>)> {
    let types: Vec<&str> = match node.get("@type")? {
        serde_json::Value::String(t) => vec![t.as_str()],
        serde_json::Value::Array(a) => a.iter().filter_map(|t| t.as_str()).collect(),
        _ => return None,
    };
    let (ty, req) = REQUIRED.iter().find(|(t, _)| types.contains(t))?;
    let empty = |k: &&str| match node.get(k) {
        None | Some(serde_json::Value::Null) => true,
        Some(serde_json::Value::String(s)) => s.trim().is_empty(),
        Some(serde_json::Value::Array(a)) => a.is_empty(),
        _ => false,
    };
    let missing: Vec<_> = req.iter().copied().filter(empty).collect();
    (!missing.is_empty()).then_some((*ty, missing))
}

/// Date W3C (`AAAA`, `AAAA-MM`, `AAAA-MM-JJ` ou date-heure avec fuseau) telle qu'attendue par `<lastmod>`.
pub(crate) fn valid_lastmod(s: &str) -> bool {
    let n = |t: &str, len: usize, lo: u32, hi: u32| {
        t.len() == len
            && t.bytes().all(|b| b.is_ascii_digit())
            && t.parse::<u32>().is_ok_and(|v| (lo..=hi).contains(&v))
    };
    let (date, time) = match s.split_once('T') {
        Some((d, t)) => (d, Some(t)),
        None => (s, None),
    };
    let mut d = date.split('-');
    let ok_date = d.next().is_some_and(|y| n(y, 4, 0, 9999))
        && d.next().is_none_or(|m| n(m, 2, 1, 12))
        && d.next().is_none_or(|j| n(j, 2, 1, 31))
        && d.next().is_none()
        && (time.is_none() || date.len() == 10);
    // ponytail: l'heure est seulement bornée (HH:MM…, Z ou ±HH:MM), pas validée en détail.
    ok_date
        && time.is_none_or(|t| {
            t.len() >= 6
                && t.as_bytes()[2] == b':'
                && (t.ends_with('Z') || t[5..].contains(['+', '-']))
        })
}

const ARTICLE_TYPES: &[&str] = &["Article", "NewsArticle", "BlogPosting"];

fn node_types(node: &serde_json::Value) -> Vec<&str> {
    match node.get("@type") {
        Some(serde_json::Value::String(t)) => vec![t.as_str()],
        Some(serde_json::Value::Array(a)) => a.iter().filter_map(|t| t.as_str()).collect(),
        _ => vec![],
    }
}

fn has_value(node: &serde_json::Value, key: &str) -> bool {
    match node.get(key) {
        None | Some(serde_json::Value::Null) => false,
        Some(serde_json::Value::String(s)) => !s.trim().is_empty(),
        Some(serde_json::Value::Array(a)) => !a.is_empty(),
        _ => true,
    }
}

/// `nosnippet` ou `max-snippet:0` (meta robots ou X-Robots-Tag) : aucun extrait, donc pas de lien
/// source dans les aperçus IA de Google.
fn blocks_snippet(directives: &str) -> bool {
    directives
        .to_lowercase()
        .split(',')
        .map(|d| d.trim().replace(' ', ""))
        .any(|d| d == "nosnippet" || d == "max-snippet:0")
}

/// Distance de Hamming maximale entre deux simhash pour parler de quasi-doublon.
const NEAR_DISTANCE: u32 = 3;
/// Au-delà, un seau de bandes est ignoré (texte commun type pied de page) pour borner le coût.
const MAX_BUCKET: usize = 200;

/// Paires de pages indexables au contenu presque identique (sans être exactement identique).
/// Bandes : 4 × 16 bits ; avec ≤ 3 bits différents, une bande au moins est égale (principe des tiroirs).
fn near_duplicates<'a>(
    pages: &[&'a CrawledPage],
    min_words: usize,
) -> Vec<(&'a CrawledPage, &'a CrawledPage)> {
    let sims: Vec<(&CrawledPage, u64)> = pages
        .iter()
        .filter(|p| p.facts.word_count.is_some_and(|w| w as usize >= min_words))
        .filter_map(|p| {
            Some((
                *p,
                u64::from_str_radix(p.facts.simhash.as_deref()?, 16).ok()?,
            ))
        })
        .collect();
    let mut buckets: HashMap<(u32, u16), Vec<usize>> = HashMap::new();
    for (i, (_, h)) in sims.iter().enumerate() {
        for band in 0..4u32 {
            buckets
                .entry((band, (h >> (band * 16)) as u16))
                .or_default()
                .push(i);
        }
    }
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for ids in buckets
        .values()
        .filter(|v| v.len() > 1 && v.len() <= MAX_BUCKET)
    {
        for (n, &i) in ids.iter().enumerate() {
            for &j in &ids[n + 1..] {
                let (a, b) = (sims[i], sims[j]);
                if (a.1 ^ b.1).count_ones() <= NEAR_DISTANCE
                    && a.0.facts.content_hash != b.0.facts.content_hash
                    && seen.insert((i, j))
                {
                    out.push((a.0, b.0));
                }
            }
        }
    }
    out
}

pub(crate) fn run(pages: &[CrawledPage], cfg: &RulesConfig, add: Add) {
    let by_url: HashMap<&str, &CrawledPage> = pages.iter().map(|p| (p.url.as_str(), p)).collect();

    for p in pages.iter().filter(|p| p.is_html) {
        let f = &p.facts;
        let ok = p.status.is_some_and(|s| (200..300).contains(&s));
        // Les faits ne sont présents que pour les audits récents.
        let measured = f.word_count.is_some();

        if f.header("x-robots-tag")
            .is_some_and(|h| h.to_lowercase().contains("noindex"))
        {
            add(
                "noindex-header",
                &p.url,
                "en-tête X-Robots-Tag noindex".into(),
            );
        }
        if !ok {
            continue;
        }

        // Longueurs (en caractères, pas en pixels : approximation de l'affichage dans les résultats).
        if let Some(t) = p.meta.title.as_deref().filter(|t| !t.is_empty()) {
            let n = t.chars().count();
            if n > cfg.title_max {
                add(
                    "title-too-long",
                    &p.url,
                    format!("{n} caractères (> {})", cfg.title_max),
                );
            } else if n < cfg.title_min {
                add(
                    "title-too-short",
                    &p.url,
                    format!("{n} caractères (< {})", cfg.title_min),
                );
            }
        }
        if let Some(d) = p.meta.description.as_deref().filter(|d| !d.is_empty()) {
            let n = d.chars().count();
            if n > cfg.description_max {
                add(
                    "description-too-long",
                    &p.url,
                    format!("{n} caractères (> {})", cfg.description_max),
                );
            } else if n < cfg.description_min {
                add(
                    "description-too-short",
                    &p.url,
                    format!("{n} caractères (< {})", cfg.description_min),
                );
            }
        }

        if p.url.scheme() == "http" {
            add(
                "insecure-http",
                &p.url,
                "page servie en HTTP, sans chiffrement".into(),
            );
        } else if p.url.scheme() == "https" {
            if f.ttfb_ms.is_some() && f.header("strict-transport-security").is_none() {
                add(
                    "hsts-missing",
                    &p.url,
                    "en-tête Strict-Transport-Security absent".into(),
                );
            }
            let mixed = f
                .images
                .iter()
                .filter(|(src, _)| src.starts_with("http://"))
                .count();
            if mixed > 0 {
                add(
                    "mixed-content",
                    &p.url,
                    format!("{mixed} image(s) chargée(s) en HTTP"),
                );
            }
        }
        if f.ttfb_ms.is_some() && f.header("x-content-type-options").is_none() {
            add(
                "nosniff-missing",
                &p.url,
                "en-tête X-Content-Type-Options absent".into(),
            );
        }
        if f.ttfb_ms.is_some_and(|ms| u64::from(ms) > cfg.slow_ms) {
            add(
                "slow-response",
                &p.url,
                format!(
                    "{} ms avant les premiers octets",
                    f.ttfb_ms.unwrap_or_default()
                ),
            );
        }

        // Contenu et contrôles statiques (sous-ensemble : pas un audit Lighthouse/axe).
        if let Some(words) = f.word_count {
            if (words as usize) < cfg.min_words {
                add(
                    "thin-content",
                    &p.url,
                    format!("{words} mots (< {})", cfg.min_words),
                );
            }
        }
        if measured {
            if f.lang.is_none() {
                add(
                    "lang-missing",
                    &p.url,
                    "attribut lang absent sur <html>".into(),
                );
            }
            if f.viewport.is_none() {
                add(
                    "viewport-missing",
                    &p.url,
                    "meta viewport absente (affichage mobile)".into(),
                );
            }
            if f.h2.is_empty() && f.word_count.is_some_and(|w| w as usize >= cfg.min_words) {
                add(
                    "h2-missing",
                    &p.url,
                    "aucun H2 sur une page de contenu".into(),
                );
            }
            let no_alt = f.images.iter().filter(|(_, alt)| alt.is_none()).count();
            if no_alt > 0 {
                add(
                    "image-alt-missing",
                    &p.url,
                    format!("{no_alt} image(s) sans attribut alt"),
                );
            }
            for (i, block) in f.json_ld.iter().enumerate() {
                match serde_json::from_str::<serde_json::Value>(block) {
                    Err(_) => add(
                        "json-ld-invalid",
                        &p.url,
                        format!(
                            "bloc JSON-LD n°{} illisible (JSON invalide ou tronqué)",
                            i + 1
                        ),
                    ),
                    Ok(v) => {
                        let mut nodes = Vec::new();
                        json_ld_nodes(&v, &mut nodes);
                        for node in &nodes {
                            let types = node_types(node);
                            if let Some(t) =
                                ["FAQPage", "HowTo"].into_iter().find(|t| types.contains(t))
                            {
                                add(
                                    "schema-faq-howto-retired",
                                    &p.url,
                                    format!("{t} : plus de résultat enrichi Google"),
                                );
                            }
                            if types.iter().any(|t| ARTICLE_TYPES.contains(t)) {
                                if !has_value(node, "author") {
                                    add(
                                        "article-author-missing",
                                        &p.url,
                                        "Article sans auteur (author)".into(),
                                    );
                                }
                                if !has_value(node, "datePublished")
                                    && !has_value(node, "dateModified")
                                {
                                    add(
                                        "article-date-missing",
                                        &p.url,
                                        "Article sans datePublished ni dateModified".into(),
                                    );
                                }
                            }
                        }
                        for (ty, missing) in nodes.into_iter().filter_map(missing_properties) {
                            add(
                                "json-ld-missing-property",
                                &p.url,
                                format!(
                                    "{ty} : propriété(s) requise(s) absente(s) : {}",
                                    missing.join(", ")
                                ),
                            );
                        }
                    }
                }
            }
        }

        let snippet_blocked = p.meta.robots.as_deref().is_some_and(blocks_snippet)
            || f.header("x-robots-tag").is_some_and(blocks_snippet);
        if snippet_blocked && !is_noindex(p) {
            add(
                "ai-snippet-blocked",
                &p.url,
                "nosnippet / max-snippet:0 : pas d'extrait, donc pas de lien source dans les aperçus IA".into(),
            );
        }
        if !f.ai_bots_blocked.is_empty() {
            add(
                "ai-crawler-blocked",
                &p.url,
                format!(
                    "robots.txt interdit le site à : {}",
                    f.ai_bots_blocked.join(", ")
                ),
            );
        }
        if f.anchor_empty > 0 {
            add(
                "anchor-empty",
                &p.url,
                format!("{} lien(s) sans texte d'ancrage accessible", f.anchor_empty),
            );
        }
        if f.anchor_generic > 0 {
            add(
                "anchor-generic",
                &p.url,
                format!(
                    "{} lien(s) à l'ancre générique (« cliquez ici »…)",
                    f.anchor_generic
                ),
            );
        }
        for (url, why) in &f.external_broken {
            add(
                "external-link-broken",
                &p.url,
                format!("lien externe {url} en échec ({why})"),
            );
        }
        if f.sitemap_lastmod
            .as_deref()
            .is_some_and(|m| !valid_lastmod(m))
        {
            add(
                "sitemap-lastmod-invalid",
                &p.url,
                format!(
                    "lastmod « {} » n'est pas une date W3C",
                    f.sitemap_lastmod.as_deref().unwrap_or_default()
                ),
            );
        }

        // AMP et microdata
        if let Some(amp) = f.amp_html.as_deref() {
            if let Some(t) = by_url.get(amp) {
                if t.status.is_some_and(|s| s >= 300) || is_noindex(t) {
                    add(
                        "amp-target-error",
                        &p.url,
                        format!("version AMP {amp} redirigée, en erreur ou en noindex"),
                    );
                }
            }
        }
        if f.is_amp && p.meta.canonical.is_none() {
            add(
                "amp-no-canonical",
                &p.url,
                "page AMP sans lien canonical vers la version classique".into(),
            );
        }
        if f.microdata_untyped > 0 {
            add(
                "microdata-no-type",
                &p.url,
                format!("{} élément(s) itemscope sans itemtype", f.microdata_untyped),
            );
        }

        // hreflang
        if !f.hreflang.is_empty() {
            for (code, _) in f.hreflang.iter().filter(|(c, _)| !valid_hreflang(c)) {
                add(
                    "hreflang-invalid-code",
                    &p.url,
                    format!("code « {code} » invalide"),
                );
            }
            if !f.hreflang.iter().any(|(_, u)| u == p.url.as_str()) {
                add(
                    "hreflang-no-self",
                    &p.url,
                    "la page ne se déclare pas elle-même dans ses hreflang".into(),
                );
            }
            for (code, target) in f.hreflang.iter().filter(|(_, u)| u != p.url.as_str()) {
                let Some(t) = by_url.get(target.as_str()) else {
                    continue;
                };
                if t.status.is_some_and(|s| s >= 300) || is_noindex(t) {
                    add(
                        "hreflang-target-error",
                        &p.url,
                        format!("{code} vise {target}, redirigée, en erreur ou en noindex"),
                    );
                } else if t.is_html && !t.facts.hreflang.iter().any(|(_, u)| u == p.url.as_str()) {
                    add(
                        "hreflang-missing-return",
                        &p.url,
                        format!("{code} : {target} ne renvoie pas vers cette page"),
                    );
                }
            }
        }
    }

    // Doublons exacts de contenu (texte visible), pages indexables avec assez de texte.
    let mut groups: HashMap<&str, Vec<&CrawledPage>> = HashMap::new();
    for p in pages.iter().filter(|p| indexable(p)) {
        if let (Some(h), Some(w)) = (p.facts.content_hash.as_deref(), p.facts.word_count) {
            if w as usize >= cfg.min_words {
                groups.entry(h).or_default().push(p);
            }
        }
    }
    for ps in groups.values().filter(|ps| ps.len() > 1) {
        for p in ps {
            add(
                "content-duplicate",
                &p.url,
                format!("même texte visible que {} autre(s) page(s)", ps.len() - 1),
            );
        }
    }

    // Quasi-doublons : chaque page est signalée avec le nombre de pages presque identiques.
    let indexable_pages: Vec<&CrawledPage> = pages.iter().filter(|p| indexable(p)).collect();
    let mut near: HashMap<&str, usize> = HashMap::new();
    for (a, b) in near_duplicates(&indexable_pages, cfg.min_words) {
        *near.entry(a.url.as_str()).or_default() += 1;
        *near.entry(b.url.as_str()).or_default() += 1;
    }
    for p in indexable_pages {
        if let Some(n) = near.get(p.url.as_str()) {
            add(
                "content-near-duplicate",
                &p.url,
                format!("texte quasi identique à {n} autre(s) page(s)"),
            );
        }
    }
}
