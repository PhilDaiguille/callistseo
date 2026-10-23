//! Extraction personnalisée et recherche dans le code source, par projet.

use callistseo_core::{
    ExtractKind, Extractor, MAX_EXTRACTED_LEN, MAX_EXTRACTED_VALUES, MAX_EXTRACTORS,
};
use regex::{Regex, RegexBuilder};
use std::collections::HashSet;

enum Compiled {
    Regex(Regex),
    Css(scraper::Selector, Option<String>),
    Contains(String),
}

#[derive(Default)]
pub struct Extractors(Vec<(String, Compiled)>);

/// Nom d'extracteur valide : il devient une clé JSON et un nom de colonne.
pub fn valid_name(name: &str) -> bool {
    (1..=30).contains(&name.len()) && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

impl Extractors {
    pub fn new(list: &[Extractor]) -> Result<Self, String> {
        if list.len() > MAX_EXTRACTORS {
            return Err(format!("extraction : {MAX_EXTRACTORS} extracteurs au plus"));
        }
        let mut names = HashSet::new();
        let mut out = Vec::new();
        for e in list {
            if !valid_name(&e.name) {
                return Err(format!(
                    "extraction « {} » : nom de 1 à 30 lettres, chiffres ou _",
                    e.name
                ));
            }
            if !names.insert(e.name.as_str()) {
                return Err(format!("extraction « {} » : nom en double", e.name));
            }
            if e.pattern.is_empty() || e.pattern.len() > 500 {
                return Err(format!(
                    "extraction « {} » : motif de 1 à 500 caractères",
                    e.name
                ));
            }
            let c = match e.kind {
                ExtractKind::Regex => Compiled::Regex(
                    RegexBuilder::new(&e.pattern)
                        .size_limit(1 << 20)
                        .build()
                        .map_err(|err| {
                            format!(
                                "extraction « {} » : {}",
                                e.name,
                                err.to_string().lines().last().unwrap_or("motif invalide")
                            )
                        })?,
                ),
                ExtractKind::Css => Compiled::Css(
                    scraper::Selector::parse(&e.pattern).map_err(|_| {
                        format!("extraction « {} » : sélecteur CSS invalide", e.name)
                    })?,
                    e.attr.clone().filter(|a| !a.is_empty()),
                ),
                ExtractKind::Contains => Compiled::Contains(e.pattern.clone()),
            };
            out.push((e.name.clone(), c));
        }
        Ok(Self(out))
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Valeurs trouvées pour chaque extracteur (bornées) ; `html` est le code source, `doc` son arbre.
    pub fn run(&self, html: &str, doc: &scraper::Html) -> Vec<(String, Vec<String>)> {
        let clip = |s: &str| -> String { s.trim().chars().take(MAX_EXTRACTED_LEN).collect() };
        let mut out = Vec::new();
        for (name, c) in &self.0 {
            let values: Vec<String> = match c {
                Compiled::Regex(re) => re
                    .captures_iter(html)
                    .filter_map(|m| m.get(1).or_else(|| m.get(0)).map(|g| clip(g.as_str())))
                    .filter(|v| !v.is_empty())
                    .take(MAX_EXTRACTED_VALUES)
                    .collect(),
                Compiled::Css(sel, attr) => doc
                    .select(sel)
                    .filter_map(|el| match attr {
                        Some(a) => el.value().attr(a).map(clip),
                        None => Some(clip(&el.text().collect::<String>())),
                    })
                    .filter(|v| !v.is_empty())
                    .take(MAX_EXTRACTED_VALUES)
                    .collect(),
                Compiled::Contains(needle) => match html.matches(needle.as_str()).count() {
                    0 => Vec::new(),
                    n => vec![n.to_string()],
                },
            };
            if !values.is_empty() {
                out.push((name.clone(), values));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ex(name: &str, kind: ExtractKind, pattern: &str, attr: Option<&str>) -> Extractor {
        Extractor {
            name: name.into(),
            kind,
            pattern: pattern.into(),
            attr: attr.map(str::to_owned),
        }
    }
    const HTML: &str = r#"<html><body><span class="price">12 €</span><span class="price"> 30 € </span>
        <a href="/a" data-x="1">A</a><script>var ga = "UA-123";</script>TODO TODO</body></html>"#;

    fn run(list: &[Extractor]) -> Vec<(String, Vec<String>)> {
        Extractors::new(list)
            .unwrap()
            .run(HTML, &scraper::Html::parse_document(HTML))
    }

    #[test]
    fn regex_css_attr_and_contains() {
        let got = run(&[
            ex("ga", ExtractKind::Regex, r#"UA-(\d+)"#, None),
            ex("whole", ExtractKind::Regex, r"\d+ €", None),
            ex("price", ExtractKind::Css, ".price", None),
            ex("href", ExtractKind::Css, "a[href]", Some("href")),
            ex("todo", ExtractKind::Contains, "TODO", None),
            ex("none", ExtractKind::Contains, "absent", None),
        ]);
        let by = |n: &str| got.iter().find(|(k, _)| k == n).map(|(_, v)| v.clone());
        assert_eq!(
            by("ga"),
            Some(vec!["123".into()]),
            "premier groupe capturant"
        );
        assert_eq!(by("whole"), Some(vec!["12 €".into(), "30 €".into()]));
        assert_eq!(
            by("price"),
            Some(vec!["12 €".into(), "30 €".into()]),
            "texte nettoyé"
        );
        assert_eq!(by("href"), Some(vec!["/a".into()]));
        assert_eq!(by("todo"), Some(vec!["2".into()]));
        assert_eq!(by("none"), None, "pas de résultat = absent");
    }

    #[test]
    fn values_are_capped() {
        let html = "<p>x</p>".repeat(50) + &"y".repeat(1000);
        let e = Extractors::new(&[
            ex("p", ExtractKind::Css, "p", None),
            ex("long", ExtractKind::Regex, "y+", None),
        ])
        .unwrap();
        let got = e.run(&html, &scraper::Html::parse_document(&html));
        assert_eq!(got[0].1.len(), MAX_EXTRACTED_VALUES);
        assert_eq!(got[1].1[0].chars().count(), MAX_EXTRACTED_LEN);
    }

    #[test]
    fn invalid_definitions_are_refused() {
        let bad = |e: Extractor| assert!(Extractors::new(&[e]).is_err());
        bad(ex("a b", ExtractKind::Contains, "x", None));
        bad(ex("", ExtractKind::Contains, "x", None));
        bad(ex("a", ExtractKind::Regex, "(", None));
        bad(ex("a", ExtractKind::Css, "div[", None));
        bad(ex("a", ExtractKind::Contains, "", None));
        assert!(
            Extractors::new(&[
                ex("a", ExtractKind::Contains, "x", None),
                ex("a", ExtractKind::Contains, "y", None)
            ])
            .is_err(),
            "doublon"
        );
        let many: Vec<_> = (0..11)
            .map(|i| ex(&format!("e{i}"), ExtractKind::Contains, "x", None))
            .collect();
        assert!(Extractors::new(&many).is_err());
    }
}
