//! Périmètre d'un audit : filtres d'URL par expressions régulières et normalisation des paramètres.

use callistseo_core::UrlScope;
use regex::{Regex, RegexBuilder};
use url::Url;

pub const MAX_PATTERNS: usize = 50;
pub const MAX_PATTERN_LEN: usize = 500;
/// Taille maximale d'un motif compilé : un motif démesuré est refusé plutôt que de saturer la mémoire.
const REGEX_SIZE_LIMIT: usize = 1 << 20;

pub struct Scope {
    include: Vec<Regex>,
    exclude: Vec<Regex>,
    strip: Vec<String>,
    max_len: Option<usize>,
}

fn compile(kind: &str, patterns: &[String]) -> Result<Vec<Regex>, String> {
    if patterns.len() > MAX_PATTERNS {
        return Err(format!("{kind} : {MAX_PATTERNS} motifs au plus"));
    }
    patterns
        .iter()
        .map(|p| {
            if p.len() > MAX_PATTERN_LEN {
                return Err(format!(
                    "{kind} : motif de plus de {MAX_PATTERN_LEN} caractères"
                ));
            }
            RegexBuilder::new(p)
                .size_limit(REGEX_SIZE_LIMIT)
                .build()
                .map_err(|e| {
                    format!(
                        "{kind} « {p} » : {}",
                        e.to_string().lines().last().unwrap_or("motif invalide")
                    )
                })
        })
        .collect()
}

impl Scope {
    pub fn new(s: &UrlScope) -> Result<Self, String> {
        if s.strip_params.len() > MAX_PATTERNS
            || s.strip_params.iter().any(|p| p.is_empty() || p.len() > 100)
        {
            return Err("paramètres à retirer : 50 au plus, de 1 à 100 caractères".into());
        }
        Ok(Self {
            include: compile("inclure", &s.include)?,
            exclude: compile("exclure", &s.exclude)?,
            strip: s.strip_params.clone(),
            max_len: s.max_url_length,
        })
    }

    /// L'URL peut-elle être suivie ? (L'URL de départ n'est jamais soumise à ce test.)
    pub fn allows(&self, url: &Url) -> bool {
        let text = url.as_str();
        self.max_len.is_none_or(|m| text.chars().count() <= m)
            && !self.exclude.iter().any(|r| r.is_match(text))
            && (self.include.is_empty() || self.include.iter().any(|r| r.is_match(text)))
    }

    /// Retire les paramètres à ignorer (`nom` exact ou `préfixe*`) ; une requête vide disparaît.
    pub fn rewrite(&self, mut url: Url) -> Url {
        if self.strip.is_empty() || url.query().is_none() {
            return url;
        }
        let drop = |name: &str| {
            self.strip.iter().any(|p| match p.strip_suffix('*') {
                Some(prefix) => name.starts_with(prefix),
                None => name == p,
            })
        };
        let kept: Vec<(String, String)> = url
            .query_pairs()
            .filter(|(k, _)| !drop(k))
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        if kept.is_empty() {
            url.set_query(None);
        } else {
            url.query_pairs_mut().clear().extend_pairs(kept);
        }
        url
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope(
        include: &[&str],
        exclude: &[&str],
        strip: &[&str],
        max: Option<usize>,
    ) -> Result<Scope, String> {
        let v = |a: &[&str]| a.iter().map(|s| s.to_string()).collect();
        Scope::new(&UrlScope {
            include: v(include),
            exclude: v(exclude),
            strip_params: v(strip),
            max_url_length: max,
            robots_override: None,
        })
    }
    fn u(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    #[test]
    fn include_exclude_and_length() {
        let s = scope(&["/blog/"], &[r"\.pdf$", "/blog/draft"], &[], Some(40)).unwrap();
        assert!(s.allows(&u("http://a.test/blog/post")));
        assert!(
            !s.allows(&u("http://a.test/shop/x")),
            "hors des motifs à inclure"
        );
        assert!(!s.allows(&u("http://a.test/blog/a.pdf")));
        assert!(
            !s.allows(&u("http://a.test/blog/draft-1")),
            "l'exclusion l'emporte"
        );
        assert!(!s.allows(&u(
            "http://a.test/blog/un-titre-beaucoup-trop-long-pour-la-limite"
        )));
        let open = scope(&[], &[], &[], None).unwrap();
        assert!(open.allows(&u("http://a.test/n-importe-quoi")));
    }

    #[test]
    fn strip_params_exact_and_prefix() {
        let s = scope(&[], &[], &["utm_*", "sid"], None).unwrap();
        assert_eq!(
            s.rewrite(u("http://a.test/p?utm_source=x&a=1&sid=9&utm_medium=y"))
                .as_str(),
            "http://a.test/p?a=1"
        );
        assert_eq!(
            s.rewrite(u("http://a.test/p?utm_source=x")).as_str(),
            "http://a.test/p"
        );
        assert_eq!(
            s.rewrite(u("http://a.test/p?a=1")).as_str(),
            "http://a.test/p?a=1"
        );
        assert_eq!(s.rewrite(u("http://a.test/p")).as_str(), "http://a.test/p");
    }

    #[test]
    fn invalid_or_excessive_input_is_refused() {
        assert!(scope(&["("], &[], &[], None).is_err());
        assert!(scope(&[&"a".repeat(501)], &[], &[], None).is_err());
        assert!(scope(&[], &["x"; 51], &[], None).is_err());
        assert!(scope(&[], &[], &[""], None).is_err());
        // Un motif compilé énorme dépasse la limite de taille.
        assert!(scope(&["(a{1000}){1000}"], &[], &[], None).is_err());
    }
}
