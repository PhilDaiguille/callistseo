//! Comparaison de deux audits : fonction pure, indépendante du stockage.

use callistseo_core::{CrawledPage, Issue};
use std::collections::{HashMap, HashSet};
use url::Url;

/// Un audit = ses pages et ses problèmes.
pub type Audit<'a> = (&'a [CrawledPage], &'a [Issue]);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub field: &'static str,
    pub before: String,
    pub after: String,
}

#[derive(Debug, Clone)]
pub struct PageChange {
    pub url: Url,
    pub changes: Vec<Change>,
}

#[derive(Debug, Default)]
pub struct Diff {
    pub added_pages: Vec<Url>,
    pub removed_pages: Vec<Url>,
    pub changed_pages: Vec<PageChange>,
    /// Présents dans `after` seulement.
    pub new_issues: Vec<Issue>,
    /// Présents dans `before` seulement.
    pub resolved_issues: Vec<Issue>,
}

/// Un problème est identifié par sa règle, son URL et son message.
fn key(i: &Issue) -> (&'static str, &Url, &str) {
    (i.rule, &i.url, &i.message)
}

fn opt<T: ToString>(v: Option<T>) -> String {
    v.map_or_else(String::new, |v| v.to_string())
}

fn page_changes(b: &CrawledPage, a: &CrawledPage) -> Vec<Change> {
    let mut out = Vec::new();
    let mut cmp = |field, before: String, after: String| {
        if before != after {
            out.push(Change {
                field,
                before,
                after,
            });
        }
    };
    cmp("status", opt(b.status), opt(a.status));
    cmp(
        "redirect_to",
        opt(b.redirect_to.as_ref()),
        opt(a.redirect_to.as_ref()),
    );
    cmp(
        "title",
        opt(b.meta.title.as_ref()),
        opt(a.meta.title.as_ref()),
    );
    cmp(
        "description",
        opt(b.meta.description.as_ref()),
        opt(a.meta.description.as_ref()),
    );
    cmp("h1", b.meta.h1.join(" | "), a.meta.h1.join(" | "));
    cmp(
        "canonical",
        opt(b.meta.canonical.as_ref()),
        opt(a.meta.canonical.as_ref()),
    );
    cmp(
        "robots",
        opt(b.meta.robots.as_ref()),
        opt(a.meta.robots.as_ref()),
    );
    cmp(
        "in_sitemap",
        b.in_sitemap.to_string(),
        a.in_sitemap.to_string(),
    );
    cmp("error", opt(b.error.as_ref()), opt(a.error.as_ref()));
    out
}

pub fn diff(before: Audit, after: Audit) -> Diff {
    let (bp, bi) = before;
    let (ap, ai) = after;
    let b_by: HashMap<&Url, &CrawledPage> = bp.iter().map(|p| (&p.url, p)).collect();
    let a_by: HashMap<&Url, &CrawledPage> = ap.iter().map(|p| (&p.url, p)).collect();

    let mut d = Diff::default();
    for p in ap {
        match b_by.get(&p.url) {
            None => d.added_pages.push(p.url.clone()),
            Some(b) => {
                let changes = page_changes(b, p);
                if !changes.is_empty() {
                    d.changed_pages.push(PageChange {
                        url: p.url.clone(),
                        changes,
                    });
                }
            }
        }
    }
    d.removed_pages = bp
        .iter()
        .filter(|p| !a_by.contains_key(&p.url))
        .map(|p| p.url.clone())
        .collect();

    let before_keys: HashSet<_> = bi.iter().map(key).collect();
    let after_keys: HashSet<_> = ai.iter().map(key).collect();
    d.new_issues = ai
        .iter()
        .filter(|i| !before_keys.contains(&key(i)))
        .cloned()
        .collect();
    d.resolved_issues = bi
        .iter()
        .filter(|i| !after_keys.contains(&key(i)))
        .cloned()
        .collect();
    d
}

#[cfg(test)]
mod tests {
    use super::*;
    use callistseo_core::{PageMeta, Severity};

    fn u(p: &str) -> Url {
        Url::parse(&format!("http://a.test{p}")).unwrap()
    }

    fn page(path: &str, status: u16, title: &str) -> CrawledPage {
        CrawledPage {
            facts: Default::default(),
            in_sitemap: false,
            is_html: true,
            meta: PageMeta {
                title: Some(title.into()),
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

    fn issue(rule: &'static str, path: &str) -> Issue {
        Issue {
            rule,
            severity: Severity::Error,
            url: u(path),
            message: "m".into(),
        }
    }

    #[test]
    fn detects_page_and_issue_differences() {
        let before = [
            page("/", 200, "Home"),
            page("/old", 200, "Old"),
            page("/p", 200, "P"),
        ];
        let after = [
            page("/", 200, "Home v2"),
            page("/new", 200, "New"),
            page("/p", 404, "P"),
        ];
        let (bi, ai) = (
            [issue("http-error", "/old"), issue("title-missing", "/p")],
            [issue("title-missing", "/p"), issue("http-error", "/p")],
        );
        let d = diff((&before, &bi), (&after, &ai));

        assert_eq!(d.added_pages, [u("/new")]);
        assert_eq!(d.removed_pages, [u("/old")]);
        let changes: Vec<_> = d
            .changed_pages
            .iter()
            .map(|c| (c.url.path(), c.changes[0].field))
            .collect();
        assert_eq!(changes, [("/", "title"), ("/p", "status")]);
        assert_eq!(
            d.changed_pages[1].changes[0],
            Change {
                field: "status",
                before: "200".into(),
                after: "404".into()
            }
        );
        assert_eq!(d.new_issues.len(), 1);
        assert_eq!(
            (d.new_issues[0].rule, d.new_issues[0].url.path()),
            ("http-error", "/p")
        );
        assert_eq!(
            (d.resolved_issues[0].rule, d.resolved_issues[0].url.path()),
            ("http-error", "/old")
        );
    }

    #[test]
    fn identical_audits_have_no_diff() {
        let pages = [page("/", 200, "Home")];
        let issues = [issue("r", "/")];
        let d = diff((&pages, &issues), (&pages, &issues));
        assert!(
            d.added_pages.is_empty() && d.removed_pages.is_empty() && d.changed_pages.is_empty()
        );
        assert!(d.new_issues.is_empty() && d.resolved_issues.is_empty());
    }
}
