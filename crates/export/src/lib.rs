use callistseo_core::{CrawledPage, Issue, Severity};
use callistseo_seo_rules::rule_info;
use serde_json::json;
use std::cmp::Reverse;
use std::collections::HashMap;
use std::fmt::Write as _;

/// Champ CSV. Le texte vient de sites tiers : une cellule qui commence par `= + - @` serait
/// interprétée comme une formule par Excel/Sheets, on la préfixe donc d'une apostrophe.
fn field(s: &str) -> String {
    let s = if s.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        format!("'{s}")
    } else {
        s.to_owned()
    };
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s
    }
}

fn row(cells: &[String]) -> String {
    let mut line = cells.join(",");
    line.push('\n');
    line
}

pub fn pages_csv(pages: &[CrawledPage]) -> String {
    let header = [
        "url",
        "depth",
        "status",
        "redirect_to",
        "title",
        "description",
        "h1_count",
        "canonical",
        "robots",
        "in_sitemap",
        "error",
    ];
    let mut out = row(&header.map(String::from));
    for p in pages {
        out += &row(&[
            field(p.url.as_str()),
            p.depth.to_string(),
            p.status.map(|s| s.to_string()).unwrap_or_default(),
            field(p.redirect_to.as_ref().map_or("", |u| u.as_str())),
            field(p.meta.title.as_deref().unwrap_or("")),
            field(p.meta.description.as_deref().unwrap_or("")),
            p.meta.h1.len().to_string(),
            field(p.meta.canonical.as_ref().map_or("", |u| u.as_str())),
            field(p.meta.robots.as_deref().unwrap_or("")),
            p.in_sitemap.to_string(),
            field(p.error.as_deref().unwrap_or("")),
        ]);
    }
    out
}

pub fn issues_csv(issues: &[Issue]) -> String {
    let mut out = row(&["severity", "rule", "url", "message"].map(String::from));
    for i in issues {
        out += &row(&[
            format!("{:?}", i.severity),
            i.rule.to_owned(),
            field(i.url.as_str()),
            field(&i.message),
        ]);
    }
    out
}

pub fn report_json(pages: &[CrawledPage], issues: &[Issue]) -> String {
    let pages: Vec<_> = pages
        .iter()
        .map(|p| {
            json!({
                "url": p.url.as_str(),
                "depth": p.depth,
                "status": p.status,
                "redirect_to": p.redirect_to.as_ref().map(|u| u.as_str()),
                "title": p.meta.title,
                "description": p.meta.description,
                "h1": p.meta.h1,
                "canonical": p.meta.canonical.as_ref().map(|u| u.as_str()),
                "robots": p.meta.robots,
                "in_sitemap": p.in_sitemap,
                "links": p.links.iter().map(|u| u.as_str()).collect::<Vec<_>>(),
                "error": p.error,
            })
        })
        .collect();
    let issues: Vec<_> = issues
        .iter()
        .map(|i| {
            json!({
                "severity": format!("{:?}", i.severity),
                "rule": i.rule,
                "url": i.url.as_str(),
                "message": i.message,
            })
        })
        .collect();
    serde_json::to_string_pretty(&json!({ "pages": pages, "issues": issues }))
        .expect("JSON sérialisable")
}

/// Secondes Unix -> `AAAA-MM-JJThh:mm:ssZ` (algorithme « civil from days », sans dépendance).
/// Maximum d'URL par fichier sitemap (protocole sitemaps.org).
pub const SITEMAP_MAX_URLS: usize = 50_000;

/// Sitemaps XML des pages indexables (`is_indexable`), triées par URL, découpées par
/// `SITEMAP_MAX_URLS`, avec les alternates hreflang. Toujours au moins un fichier.
/// `lastmod` est volontairement omis : ni le serveur ni le crawl ne le garantissent fiable.
pub fn sitemap_parts(pages: &[CrawledPage]) -> Vec<String> {
    let mut entries: Vec<&CrawledPage> = pages
        .iter()
        .filter(|p| callistseo_seo_rules::is_indexable(p))
        .collect();
    entries.sort_by(|a, b| a.url.cmp(&b.url));
    let render = |chunk: &[&CrawledPage]| {
        let mut x = String::from(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\" xmlns:xhtml=\"http://www.w3.org/1999/xhtml\">\n",
        );
        for p in chunk {
            let _ = write!(x, "  <url>\n    <loc>{}</loc>\n", esc(p.url.as_str()));
            for (lang, href) in &p.facts.hreflang {
                let _ = writeln!(
                    x,
                    "    <xhtml:link rel=\"alternate\" hreflang=\"{}\" href=\"{}\"/>",
                    esc(lang),
                    esc(href)
                );
            }
            x.push_str("  </url>\n");
        }
        x.push_str("</urlset>\n");
        x
    };
    if entries.is_empty() {
        return vec![render(&[])];
    }
    entries.chunks(SITEMAP_MAX_URLS).map(render).collect()
}

/// Index qui référence les fichiers `part_urls` (URL absolues, à la charge de l'appelant).
pub fn sitemap_index(part_urls: &[String]) -> String {
    let mut x = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<sitemapindex xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n",
    );
    for u in part_urls {
        let _ = writeln!(x, "  <sitemap><loc>{}</loc></sitemap>", esc(u));
    }
    x.push_str("</sitemapindex>\n");
    x
}

pub fn iso_utc(secs: i64) -> String {
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// En-têtes du rapport HTML.
pub struct ReportMeta<'a> {
    pub title: &'a str,
    pub start_url: &'a str,
    pub generated_at: &'a str,
}

/// Lignes affichées au plus par règle / dans le tableau des URL (le rapport reste lisible et léger).
const RULE_ROWS_MAX: usize = 200;
const PAGE_ROWS_MAX: usize = 5_000;

/// Échappement HTML : titres, descriptions et URL viennent de sites tiers.
fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

fn severity_label(s: Severity) -> (&'static str, &'static str) {
    match s {
        Severity::Error => ("error", "Erreur"),
        Severity::Warning => ("warning", "Avertissement"),
        Severity::Info => ("info", "Info"),
    }
}

const REPORT_CSS: &str = "\
:root{--bg:#FAF7EF;--card:#FFFDF8;--fg:#1D1C19;--muted:#6A6457;--line:rgba(29,28,25,.16);--soft:#E8E2D6;--ink:#1D1C19;--accent:#C5CEEE;--err:#B3261E;--err-bg:#F9DCD8;--warn:#1D1C19;--warn-bg:#F7E7BC}\
@media (prefers-color-scheme:dark){:root{--bg:#1B1A17;--card:#25231F;--fg:#F3EFE4;--muted:#A9A394;--line:rgba(243,239,228,.16);--soft:#34312B;--ink:#EDE7D8;--accent:#C5CEEE;--err:#FF8A80;--err-bg:#4A1F1C;--warn:#F3EFE4;--warn-bg:#4A3A12}}\
*{box-sizing:border-box}body{margin:0;background:var(--bg);color:var(--fg);font:15px/1.5 Inter,system-ui,-apple-system,Segoe UI,sans-serif;background-image:radial-gradient(color-mix(in oklab,var(--fg) 9%,transparent) 1px,transparent 1px);background-size:18px 18px}\
main{max-width:1100px;margin:0 auto;padding:32px 20px 64px}h1,h2{font-family:Nunito,ui-rounded,system-ui,sans-serif;letter-spacing:-.01em}h1{font-size:30px;font-weight:800;margin:0 0 4px}h2{font-size:21px;font-weight:800;margin:36px 0 12px}\
.meta{color:var(--muted);word-break:break-all;margin:0}\
.cards{display:flex;flex-wrap:wrap;gap:12px;margin:24px 0 0}.card{background:var(--card);border:1px solid var(--line);border-radius:16px;padding:10px 16px;min-width:120px}\
.card b{display:block;font:800 26px Nunito,ui-rounded,system-ui,sans-serif}.card span{color:var(--muted);font-size:13px}\
.card.error{background:var(--err-bg);border-color:var(--err)}.card.error b{color:var(--err)}\
details{background:var(--card);border:1px solid var(--line);border-radius:16px;margin:10px 0;padding:0 14px}summary{cursor:pointer;padding:12px 0;display:flex;gap:10px;align-items:center;flex-wrap:wrap}\
.badge{display:inline-block;border-radius:99px;padding:1px 10px;font-size:12px;font-weight:700;background:var(--soft)}\
.badge.error{background:var(--err-bg);color:var(--err)}.badge.warning{background:var(--warn-bg);color:var(--warn)}\
.count{color:var(--muted)}.help{color:var(--muted);margin:0 0 10px;font-size:14px}\
table{width:100%;border-collapse:collapse;font-size:14px;margin-bottom:14px}th,td{text-align:left;padding:6px 8px;border-top:1px solid var(--line);vertical-align:top}\
th{color:var(--fg);font-weight:700;border-top:0}td.url{font-family:ui-monospace,Menlo,monospace;font-size:12.5px;word-break:break-all}\
.num{text-align:right;white-space:nowrap}.s-error{color:var(--err);font-weight:700}.s-redirect{color:var(--warn);font-weight:700}\
.note{color:var(--muted);font-size:13px}footer{margin-top:48px;color:var(--muted);font-size:13px}";

/// Rapport HTML autonome (un seul fichier, aucun script, aucune ressource externe).
/// La politique de sécurité embarquée interdit tout script, même si un contenu échappait au filtrage.
pub fn report_html(meta: &ReportMeta, pages: &[CrawledPage], issues: &[Issue]) -> String {
    let count = |s| issues.iter().filter(|i| i.severity == s).count();
    let mut h = String::new();
    let _ = write!(
        h,
        "<!doctype html><html lang=\"fr\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
         <meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; style-src 'unsafe-inline'\">\
         <title>Audit SEO — {title}</title><style>{REPORT_CSS}</style></head><body><main>\
         <h1>Audit SEO — {title}</h1><p class=\"meta\">{url} · {date}</p>\
         <div class=\"cards\">\
         <div class=\"card\"><b>{pages}</b><span>URL explorées</span></div>\
         <div class=\"card error\"><b>{errors}</b><span>Erreurs</span></div>\
         <div class=\"card\"><b>{warnings}</b><span>Avertissements</span></div>\
         <div class=\"card\"><b>{infos}</b><span>Infos</span></div></div>",
        title = esc(meta.title),
        url = esc(meta.start_url),
        date = esc(meta.generated_at),
        pages = pages.len(),
        errors = count(Severity::Error),
        warnings = count(Severity::Warning),
        infos = count(Severity::Info),
    );

    h.push_str("<h2>Problèmes</h2>");
    if issues.is_empty() {
        h.push_str("<p>Aucun problème détecté.</p>");
    }
    let mut by_rule: HashMap<&str, Vec<&Issue>> = HashMap::new();
    for i in issues {
        by_rule.entry(i.rule).or_default().push(i);
    }
    let mut groups: Vec<(&str, Vec<&Issue>)> = by_rule.into_iter().collect();
    groups.sort_by(|a, b| {
        (a.1[0].severity, Reverse(a.1.len()), a.0).cmp(&(b.1[0].severity, Reverse(b.1.len()), b.0))
    });
    for (rule, list) in &groups {
        let (class, severity) = severity_label(list[0].severity);
        let (label, help) = rule_info(rule).map_or((*rule, ""), |r| (r.label, r.help));
        let _ = write!(
            h,
            "<details{open}><summary><span class=\"badge {class}\">{severity}</span><strong>{label}</strong>\
             <span class=\"count\">{n}</span></summary><p class=\"help\">{help}</p>\
             <table><thead><tr><th>URL</th><th>Détail</th></tr></thead><tbody>",
            open = if list[0].severity == Severity::Error { " open" } else { "" },
            label = esc(label),
            help = esc(help),
            n = list.len(),
        );
        for i in list.iter().take(RULE_ROWS_MAX) {
            let _ = write!(
                h,
                "<tr><td class=\"url\">{}</td><td>{}</td></tr>",
                esc(i.url.as_str()),
                esc(&i.message)
            );
        }
        h.push_str("</tbody></table>");
        if list.len() > RULE_ROWS_MAX {
            let _ = write!(
                h,
                "<p class=\"note\">… et {} autres.</p>",
                list.len() - RULE_ROWS_MAX
            );
        }
        h.push_str("</details>");
    }

    h.push_str(
        "<h2>URL explorées</h2><table><thead><tr><th>Statut</th><th class=\"num\">Prof.</th>\
         <th>URL</th><th>Title</th></tr></thead><tbody>",
    );
    for p in pages.iter().take(PAGE_ROWS_MAX) {
        let (status, class) = match (p.status, p.error.as_deref()) {
            (Some(s), _) => (
                s.to_string(),
                if s >= 400 {
                    "s-error"
                } else if s >= 300 {
                    "s-redirect"
                } else {
                    ""
                },
            ),
            (None, Some(e)) if e.contains("robots.txt") => ("robots".to_owned(), ""),
            (None, _) => ("ERR".to_owned(), "s-error"),
        };
        let _ = write!(
            h,
            "<tr><td class=\"{class}\">{status}</td><td class=\"num\">{}</td><td class=\"url\">{}</td><td>{}</td></tr>",
            p.depth,
            esc(p.url.as_str()),
            esc(p.meta.title.as_deref().unwrap_or("")),
        );
    }
    h.push_str("</tbody></table>");
    if pages.len() > PAGE_ROWS_MAX {
        let _ = write!(
            h,
            "<p class=\"note\">… et {} autres URL (voir l'export CSV pour la liste complète).</p>",
            pages.len() - PAGE_ROWS_MAX
        );
    }
    h.push_str("<footer>Rapport généré par CallistSEO. Les constats sont techniques : ce ne sont pas des pénalités certaines des moteurs de recherche.</footer></main></body></html>");
    h
}

#[cfg(test)]
mod tests {
    use super::*;
    use callistseo_core::{PageMeta, Severity};
    use url::Url;

    fn page(title: &str) -> CrawledPage {
        CrawledPage {
            facts: Default::default(),
            in_sitemap: true,
            is_html: true,
            meta: PageMeta {
                title: Some(title.into()),
                ..PageMeta::default()
            },
            url: Url::parse("http://a.test/x?a=1,2").unwrap(),
            depth: 1,
            status: Some(200),
            redirect_to: None,
            links: vec![],
            error: None,
        }
    }

    #[test]
    fn csv_escapes_and_neutralizes_formulas() {
        let csv = pages_csv(&[page("=HYPERLINK(\"http://evil\")"), page("a, \"b\"")]);
        let lines: Vec<_> = csv.lines().collect();
        assert!(
            lines[1].contains("\"'=HYPERLINK(\"\"http://evil\"\")\""),
            "{}",
            lines[1]
        );
        assert!(lines[2].contains("\"a, \"\"b\"\"\""), "{}", lines[2]);
        assert!(lines[1].starts_with("\"http://a.test/x?a=1,2\",1,200,"));
    }

    fn issue(rule: &'static str, severity: Severity, message: &str) -> Issue {
        Issue {
            rule,
            severity,
            url: Url::parse("http://a.test/x").unwrap(),
            message: message.into(),
        }
    }

    const META: ReportMeta = ReportMeta {
        title: "Mon site",
        start_url: "http://a.test/",
        generated_at: "2026-10-20T12:00:00Z",
    };

    #[test]
    fn html_report_escapes_untrusted_content_and_forbids_scripts() {
        let page = page("<script>alert(1)</script>");
        let issues = [issue(
            "http-error",
            Severity::Error,
            "\"><img src=x onerror=alert(1)>",
        )];
        let html = report_html(
            &ReportMeta {
                title: "<b>x</b>",
                ..META
            },
            &[page],
            &issues,
        );
        assert!(
            !html.contains("<script") && !html.contains("<img") && !html.contains("<b>x"),
            "{html}"
        );
        assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
        assert!(html.contains("&quot;&gt;&lt;img src=x onerror=alert(1)&gt;"));
        assert!(html.contains("Content-Security-Policy") && html.contains("default-src 'none'"));
    }

    #[test]
    fn html_report_groups_by_rule_with_explanations() {
        let issues = [
            issue("h1-missing", Severity::Warning, "aucun H1"),
            issue("http-error", Severity::Error, "statut HTTP 404"),
            issue("http-error", Severity::Error, "statut HTTP 500"),
        ];
        let html = report_html(&META, &[page("T")], &issues);
        assert!(html.contains("Audit SEO — Mon site") && html.contains("2026-10-20T12:00:00Z"));
        let (err, warn) = (
            html.find("Erreur HTTP").unwrap(),
            html.find("H1 absent").unwrap(),
        );
        assert!(err < warn, "les erreurs viennent avant les avertissements");
        assert!(html.contains("<details open>")); // les erreurs sont dépliées
        assert!(
            html.contains("inaccessible aux visiteurs"),
            "explication de la règle"
        );
        assert!(
            html.contains("<b>2</b><span>Erreurs</span>") || html.contains("<b>2</b><span>Erreurs")
        );
        assert!(report_html(&META, &[], &[]).contains("Aucun problème détecté."));
    }

    #[test]
    fn html_report_truncates_long_lists() {
        let issues: Vec<_> = (0..300)
            .map(|_| issue("redirect", Severity::Info, "m"))
            .collect();
        let html = report_html(&META, &[], &issues);
        assert!(html.contains("… et 100 autres."));
        assert_eq!(html.matches("<td class=\"url\">").count(), 200);
    }

    #[test]
    fn iso_dates() {
        assert_eq!(iso_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso_utc(1_700_000_000), "2023-11-14T22:13:20Z");
        assert_eq!(iso_utc(951_782_400), "2000-02-29T00:00:00Z"); // année bissextile séculaire
    }

    #[test]
    fn json_has_pages_and_issues() {
        let issue = Issue {
            rule: "r",
            severity: Severity::Warning,
            url: Url::parse("http://a.test/").unwrap(),
            message: "m".into(),
        };
        let v: serde_json::Value =
            serde_json::from_str(&report_json(&[page("t")], &[issue])).unwrap();
        assert_eq!(v["pages"][0]["title"], "t");
        assert_eq!(v["issues"][0]["severity"], "Warning");
        assert!(issues_csv(&[]).starts_with("severity,rule,url,message"));
    }

    #[test]
    fn sitemap_lists_only_indexable_pages_sorted_and_escaped() {
        let mk = |path: &str, status: u16| {
            let mut p = page("t");
            p.url = url::Url::parse(&format!("http://a.test{path}")).unwrap();
            p.status = Some(status);
            p
        };
        let mut noindex = mk("/noindex", 200);
        noindex.meta.robots = Some("noindex".into());
        let mut canon = mk("/canon", 200);
        canon.meta.canonical = Some(url::Url::parse("http://a.test/autre").unwrap());
        let mut fr = mk("/fr?a=1&b='2'", 200);
        fr.facts.hreflang = vec![("en".into(), "http://a.test/en".into())];
        let parts = sitemap_parts(&[
            mk("/z", 200),
            noindex,
            canon,
            mk("/404", 404),
            fr,
            mk("/a", 200),
        ]);
        assert_eq!(parts.len(), 1);
        let x = &parts[0];
        let locs: Vec<_> = x
            .lines()
            .filter(|l| l.contains("<loc>"))
            .map(str::trim)
            .collect();
        assert_eq!(
            locs,
            [
                "<loc>http://a.test/a</loc>",
                "<loc>http://a.test/fr?a=1&amp;b=%272%27</loc>",
                "<loc>http://a.test/z</loc>"
            ]
        );
        assert!(
            x.contains(r#"<xhtml:link rel="alternate" hreflang="en" href="http://a.test/en"/>"#)
        );
        assert!(!x.contains("noindex") && !x.contains("canon") && !x.contains("404"));
        assert_eq!(
            sitemap_parts(&[]).len(),
            1,
            "un site sans page indexable donne un fichier vide valide"
        );
        assert!(sitemap_index(&["http://a.test/s?part=1".into()])
            .contains("<sitemap><loc>http://a.test/s?part=1</loc></sitemap>"));
    }
}
