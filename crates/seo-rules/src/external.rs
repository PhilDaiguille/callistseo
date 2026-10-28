//! Règles sur les mesures des intégrations (PageSpeed, Search Console), appliquées après coup : ces
//! mesures arrivent une fois l'audit enregistré. Une mesure absente ne déclenche jamais de règle.

use crate::rule_info;
use callistseo_core::Issue;
use url::Url;

/// Seuils « mauvais » des Core Web Vitals (web.dev) : au-delà, l'expérience est jugée mauvaise.
pub const LCP_POOR_MS: i64 = 4000;
pub const CLS_POOR: f64 = 0.25;
pub const INP_POOR_MS: i64 = 500;
/// Score Lighthouse « faible » (rouge dans PageSpeed Insights).
pub const SCORE_LOW: i64 = 50;

/// Mesures externes d'une page de l'audit.
#[derive(Debug, Clone)]
pub struct Measured {
    pub url: Url,
    /// La page devrait être dans l'index (HTML 2xx, pas de noindex ni de canonical ailleurs).
    pub indexable: bool,
    pub psi_score: Option<i64>,
    pub lcp_ms: Option<i64>,
    pub cls: Option<f64>,
    pub inp_ms: Option<i64>,
    /// Impressions Search Console sur 28 jours ; `None` si Search Console n'a pas tourné.
    pub gsc_impressions: Option<i64>,
}

impl Measured {
    /// Page sans aucune mesure (à compléter champ par champ).
    pub fn new(url: Url, indexable: bool) -> Self {
        Self {
            url,
            indexable,
            psi_score: None,
            lcp_ms: None,
            cls: None,
            inp_ms: None,
            gsc_impressions: None,
        }
    }
}

pub fn external_issues(pages: &[Measured]) -> Vec<Issue> {
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
        if let Some(v) = p.lcp_ms.filter(|v| *v > LCP_POOR_MS) {
            add(
                "cwv-lcp-poor",
                &p.url,
                format!("LCP {:.1} s (mauvais au-delà de 4 s)", v as f64 / 1000.0),
            );
        }
        if let Some(v) = p.cls.filter(|v| *v > CLS_POOR) {
            add(
                "cwv-cls-poor",
                &p.url,
                format!("CLS {v:.2} (mauvais au-delà de 0,25)"),
            );
        }
        if let Some(v) = p.inp_ms.filter(|v| *v > INP_POOR_MS) {
            add(
                "cwv-inp-poor",
                &p.url,
                format!("INP {v} ms (mauvais au-delà de 500 ms)"),
            );
        }
        if let Some(v) = p.psi_score.filter(|v| *v < SCORE_LOW) {
            add("psi-score-low", &p.url, format!("score PageSpeed {v}/100"));
        }
        if p.indexable && p.gsc_impressions == Some(0) {
            add(
                "gsc-no-impressions",
                &p.url,
                "aucune impression dans Google sur 28 jours".into(),
            );
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thresholds_and_missing_values() {
        let u = |p: &str| Url::parse(&format!("https://a.test{p}")).unwrap();
        let pages = [
            Measured {
                url: u("/lent"),
                indexable: true,
                psi_score: Some(30),
                lcp_ms: Some(4100),
                cls: Some(0.3),
                inp_ms: Some(600),
                gsc_impressions: Some(0),
            },
            // Juste aux seuils : rien.
            Measured {
                url: u("/limite"),
                indexable: true,
                psi_score: Some(50),
                lcp_ms: Some(4000),
                cls: Some(0.25),
                inp_ms: Some(500),
                gsc_impressions: Some(1),
            },
            // Non mesurée, ou non indexable sans impression : rien.
            Measured::new(u("/rien"), true),
            Measured {
                gsc_impressions: Some(0),
                ..Measured::new(u("/noindex"), false)
            },
        ];
        let got: Vec<_> = external_issues(&pages)
            .iter()
            .map(|i| (i.rule, i.url.path().to_owned()))
            .collect();
        let lent = |r| (r, "/lent".to_owned());
        assert_eq!(
            got,
            [
                lent("cwv-lcp-poor"),
                lent("cwv-cls-poor"),
                lent("cwv-inp-poor"),
                lent("psi-score-low"),
                lent("gsc-no-impressions")
            ]
        );
    }
}
