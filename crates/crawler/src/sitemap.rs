/// URL des sitemaps déclarés dans un robots.txt (`Sitemap: <url>`).
pub fn declared(robots_txt: &str) -> Vec<String> {
    robots_txt
        .lines()
        .filter_map(|l| {
            let (k, v) = l.split_once(':')?;
            k.trim()
                .eq_ignore_ascii_case("sitemap")
                .then(|| v.trim().to_owned())
        })
        .filter(|v| !v.is_empty())
        .collect()
}

/// Renvoie (est_un_index, contenu des balises `<loc>`).
// ponytail: extraction textuelle de <loc>, sans parseur XML (le .xml.gz est décompressé en amont, dans le client). Suffisant pour les sitemaps standard.
#[cfg(test)]
pub fn parse(xml: &str) -> (bool, Vec<String>) {
    let (is_index, entries) = parse_entries(xml);
    (is_index, entries.into_iter().map(|(l, _)| l).collect())
}

/// Comme `parse`, avec le `<lastmod>` de chaque entrée (cherché avant le `<loc>` suivant).
pub fn parse_entries(xml: &str) -> (bool, Vec<(String, Option<String>)>) {
    let is_index = xml.contains("<sitemapindex");
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(i) = rest.find("<loc>") {
        rest = &rest[i + 5..];
        let Some(j) = rest.find("</loc>") else { break };
        let raw = rest[..j].trim();
        let raw = raw
            .strip_prefix("<![CDATA[")
            .and_then(|r| r.strip_suffix("]]>"))
            .unwrap_or(raw);
        let loc = unescape(raw.trim());
        rest = &rest[j + 6..];
        let entry_end = rest.find("<loc>").unwrap_or(rest.len());
        let lastmod = rest[..entry_end].find("<lastmod>").and_then(|a| {
            let tail = &rest[a + 9..entry_end];
            Some(tail[..tail.find("</lastmod>")?].trim().to_owned())
        });
        out.push((loc, lastmod));
    }
    (is_index, out)
}

fn unescape(s: &str) -> String {
    // `&amp;` en dernier pour ne pas double-décoder.
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_urlset_index_and_robots_lines() {
        let (idx, locs) = parse(
            r#"<urlset><url><loc> http://a.test/x?a=1&amp;b=2 </loc></url><url><loc><![CDATA[http://a.test/y]]></loc></url></urlset>"#,
        );
        assert!(!idx);
        assert_eq!(locs, ["http://a.test/x?a=1&b=2", "http://a.test/y"]);
        let (idx, locs) = parse(
            "<sitemapindex><sitemap><loc>http://a.test/s1.xml</loc></sitemap></sitemapindex>",
        );
        assert!(idx && locs == ["http://a.test/s1.xml"]);
        assert_eq!(
            declared("User-agent: *\nsitemap: http://a.test/s.xml\nSitemap:\n"),
            ["http://a.test/s.xml"]
        );
        let (_, e) = parse_entries(
            "<urlset><url><loc>http://a.test/a</loc><lastmod>2026-01-02</lastmod></url><url><loc>http://a.test/b</loc></url></urlset>",
        );
        assert_eq!(e[0].1.as_deref(), Some("2026-01-02"));
        assert_eq!(e[1].1, None);
    }
}
