//! Intégrations externes, désactivées par défaut et activées par projet avec les clés de
//! l'utilisateur : PageSpeed Insights (Core Web Vitals), Search Console et GA4 (compte de service).
//!
//! Ce qui part chez Google : les URL échantillonnées (PageSpeed) ; la propriété demandée et un
//! jeton signé par le compte de service (Search Console, GA4). Rien d'autre (ni contenu, ni
//! identifiants des sites protégés). Elles tournent après l'enregistrement de l'audit : une panne
//! chez Google n'empêche jamais d'avoir les résultats du crawl.

use base64::Engine;
use serde::{Deserialize, Serialize};

/// Configuration non secrète (dans la configuration du projet). Une intégration présente est
/// activée : l'interface demande d'accepter ce qui part chez qui avant de l'enregistrer.
#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct IntegrationsConfig {
    pub psi: Option<PsiConfig>,
    pub gsc: Option<GscConfig>,
    pub ga4: Option<Ga4Config>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PsiConfig {
    /// `mobile` (défaut, celui de l'index Google) ou `desktop`.
    #[serde(default = "mobile")]
    pub strategy: String,
    /// Nombre de pages mesurées par audit (plafonné à `PSI_MAX_URLS`).
    #[serde(default = "psi_default_urls")]
    pub max_urls: usize,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GscConfig {
    /// Propriété Search Console : `sc-domain:exemple.fr` ou `https://www.exemple.fr/`.
    pub property: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Ga4Config {
    /// Identifiant numérique de la propriété GA4 (pas le `G-…` du flux web).
    pub property_id: String,
}

fn mobile() -> String {
    "mobile".into()
}
fn psi_default_urls() -> usize {
    20
}

/// Plafond de pages mesurées par PageSpeed (chaque mesure prend 10 à 30 s chez Google).
pub const PSI_MAX_URLS: usize = 100;

impl IntegrationsConfig {
    pub fn validate(&self) -> Result<(), String> {
        if let Some(p) = &self.psi {
            if !matches!(p.strategy.as_str(), "mobile" | "desktop") {
                return Err("PageSpeed : stratégie mobile ou desktop".into());
            }
            if !(1..=PSI_MAX_URLS).contains(&p.max_urls) {
                return Err(format!("PageSpeed : de 1 à {PSI_MAX_URLS} pages par audit"));
            }
        }
        if let Some(g) = &self.gsc {
            let p = g.property.trim();
            let ok = p
                .strip_prefix("sc-domain:")
                .is_some_and(|d| !d.is_empty() && !d.contains('/'))
                || url::Url::parse(p)
                    .is_ok_and(|u| matches!(u.scheme(), "http" | "https") && p.ends_with('/'));
            if !ok || p.len() > 300 {
                return Err(
                    "Search Console : propriété `sc-domain:exemple.fr` ou `https://www.exemple.fr/` (avec la barre finale)"
                        .into(),
                );
            }
        }
        if let Some(g) = &self.ga4 {
            let id = g.property_id.trim();
            if id.is_empty() || id.len() > 20 || !id.bytes().all(|b| b.is_ascii_digit()) {
                return Err("GA4 : identifiant numérique de la propriété (Admin → Détails de la propriété), pas l'identifiant G-…".into());
            }
        }
        Ok(())
    }

    pub fn is_empty(&self) -> bool {
        self.psi.is_none() && self.gsc.is_none() && self.ga4.is_none()
    }
}

/// Clés (chiffrées en base, jamais renvoyées).
#[derive(Serialize, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct IntegrationSecrets {
    /// Clé d'API PageSpeed Insights (facultative : sans elle, le quota anonyme est très bas).
    pub psi_key: Option<String>,
    /// Fichier JSON du compte de service Google (Search Console et GA4).
    pub service_account: Option<String>,
}

/// Adresses des services (modifiables pour les tests).
#[derive(Clone, Debug)]
pub struct Endpoints {
    pub psi: String,
    pub gsc: String,
    pub ga4: String,
    /// Remplace le `token_uri` du compte de service (tests seulement).
    pub token: Option<String>,
}

impl Default for Endpoints {
    fn default() -> Self {
        Self {
            psi: "https://pagespeedonline.googleapis.com/pagespeedonline/v5/runPagespeed".into(),
            gsc: "https://www.googleapis.com/webmasters/v3/sites".into(),
            ga4: "https://analyticsdata.googleapis.com/v1beta/properties".into(),
            token: None,
        }
    }
}

pub const SCOPE_GSC: &str = "https://www.googleapis.com/auth/webmasters.readonly";
pub const SCOPE_GA4: &str = "https://www.googleapis.com/auth/analytics.readonly";

/// Client HTTP des intégrations : PageSpeed peut mettre jusqu'à 120 s à répondre.
pub fn http_client(user_agent: &str) -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(130))
        .user_agent(user_agent)
        .build()
        .map_err(|e| e.to_string())
}

/// Message d'erreur d'une réponse Google (`error.message`), sinon le statut.
async fn google_error(resp: reqwest::Response) -> String {
    let status = resp.status().as_u16();
    let body: serde_json::Value = resp.json().await.unwrap_or_default();
    let msg = body["error"]["message"]
        .as_str()
        .or_else(|| body["error_description"].as_str())
        .or_else(|| body["error"].as_str())
        .unwrap_or("réponse inattendue");
    let msg: String = msg.chars().take(300).collect();
    format!("HTTP {status} : {msg}")
}

// ---- PageSpeed Insights ---------------------------------------------------------------------

/// Mesures d'une réponse `runPagespeed`. Données de terrain (CrUX) de la page elle-même si Google
/// en a ; sinon (rien, ou seulement celles de l'origine) : laboratoire Lighthouse, sans INP.
pub fn parse_psi(v: &serde_json::Value) -> Result<callistseo_storage::PsiMetrics, String> {
    let lh = &v["lighthouseResult"];
    if let Some(msg) = lh["runtimeError"]["message"].as_str() {
        return Err(format!("Lighthouse : {msg}"));
    }
    if lh.is_null() {
        return Err("réponse PageSpeed sans résultat Lighthouse".into());
    }
    let score = lh["categories"]["performance"]["score"]
        .as_f64()
        .map(|s| (s * 100.0).round() as i64);
    let field = &v["loadingExperience"];
    let page_field =
        !field["metrics"].is_null() && field["origin_fallback"].as_bool() != Some(true);
    let metric = |key: &str| &field["metrics"][key];
    let lab = |audit: &str| lh["audits"][audit]["numericValue"].as_f64();
    if page_field {
        let cls = metric("CUMULATIVE_LAYOUT_SHIFT_SCORE");
        // Le centile du CLS est un entier ×100 (5 = 0,05) ; les tranches le confirment (max = 10).
        let cls_value = cls["percentile"].as_f64().map(|p| {
            let scaled = cls["distributions"][0]["max"].as_f64() == Some(10.0) || p > 1.0;
            if scaled {
                p / 100.0
            } else {
                p
            }
        });
        return Ok(callistseo_storage::PsiMetrics {
            score,
            lcp_ms: metric("LARGEST_CONTENTFUL_PAINT_MS")["percentile"].as_i64(),
            cls: cls_value.or_else(|| lab("cumulative-layout-shift")),
            inp_ms: metric("INTERACTION_TO_NEXT_PAINT")["percentile"].as_i64(),
            source: "terrain",
        });
    }
    Ok(callistseo_storage::PsiMetrics {
        score,
        lcp_ms: lab("largest-contentful-paint").map(|v| v.round() as i64),
        cls: lab("cumulative-layout-shift"),
        inp_ms: None,
        source: "labo",
    })
}

pub async fn psi_one(
    http: &reqwest::Client,
    ep: &Endpoints,
    key: &str,
    url: &str,
    strategy: &str,
) -> Result<callistseo_storage::PsiMetrics, String> {
    let resp = http
        .get(&ep.psi)
        .query(&[
            ("url", url),
            ("strategy", &strategy.to_ascii_uppercase()),
            ("category", "PERFORMANCE"),
            ("key", key),
        ])
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(google_error(resp).await);
    }
    parse_psi(&resp.json().await.map_err(|e| e.to_string())?)
}

// ---- jeton d'accès --------------------------------------------------------------------------

pub async fn access_token(
    http: &reqwest::Client,
    ep: &Endpoints,
    sa: &ServiceAccount,
    scope: &str,
    now: i64,
) -> Result<String, String> {
    let assertion = sa.assertion(scope, now)?;
    let uri = ep.token.as_deref().unwrap_or(&sa.token_uri);
    let body = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer")
        .append_pair("assertion", &assertion)
        .finish();
    let resp = http
        .post(uri)
        .header(
            reqwest::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .body(body)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("jeton refusé ({})", google_error(resp).await));
    }
    let v: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    v["access_token"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| "jeton absent de la réponse".into())
}

// ---- Search Console -------------------------------------------------------------------------

pub fn parse_gsc(v: &serde_json::Value) -> Vec<(String, callistseo_storage::GscMetrics)> {
    v["rows"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .filter_map(|r| {
                    Some((
                        r["keys"][0].as_str()?.to_owned(),
                        callistseo_storage::GscMetrics {
                            clicks: r["clicks"].as_f64()? as i64,
                            impressions: r["impressions"].as_f64()? as i64,
                            ctr: r["ctr"].as_f64().unwrap_or(0.0),
                            position: r["position"].as_f64().unwrap_or(0.0),
                        },
                    ))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Lignes par page sur 28 jours (paquets de 25 000, 4 au plus).
pub async fn gsc_rows(
    http: &reqwest::Client,
    ep: &Endpoints,
    token: &str,
    property: &str,
    (start, end): (&str, &str),
) -> Result<Vec<(String, callistseo_storage::GscMetrics)>, String> {
    const PAGE: usize = 25_000;
    let site: String = url::form_urlencoded::byte_serialize(property.trim().as_bytes()).collect();
    let mut all = Vec::new();
    for page in 0..4 {
        let resp = http
            .post(format!("{}/{site}/searchAnalytics/query", ep.gsc))
            .bearer_auth(token)
            .json(&serde_json::json!({
                "startDate": start, "endDate": end, "dimensions": ["page"], "type": "web",
                "rowLimit": PAGE, "startRow": page * PAGE, "dataState": "final",
            }))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(google_error(resp).await);
        }
        let rows = parse_gsc(&resp.json().await.map_err(|e| e.to_string())?);
        let n = rows.len();
        all.extend(rows);
        if n < PAGE {
            break;
        }
    }
    Ok(all)
}

// ---- GA4 ------------------------------------------------------------------------------------

/// Lignes `pagePath` → mesures (valeurs envoyées en texte par l'API).
pub fn parse_ga4(v: &serde_json::Value) -> Vec<(String, callistseo_storage::Ga4Metrics)> {
    let names: Vec<&str> = v["metricHeaders"]
        .as_array()
        .map(|h| h.iter().filter_map(|m| m["name"].as_str()).collect())
        .unwrap_or_default();
    let col = |row: &serde_json::Value, name: &str| -> f64 {
        names
            .iter()
            .position(|n| *n == name)
            .and_then(|i| row["metricValues"][i]["value"].as_str())
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.0)
    };
    v["rows"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .filter_map(|r| {
                    Some((
                        r["dimensionValues"][0]["value"].as_str()?.to_owned(),
                        callistseo_storage::Ga4Metrics {
                            sessions: col(r, "sessions") as i64,
                            views: col(r, "screenPageViews") as i64,
                            engagement: col(r, "engagementRate"),
                        },
                    ))
                })
                .collect()
        })
        .unwrap_or_default()
}

pub async fn ga4_rows(
    http: &reqwest::Client,
    ep: &Endpoints,
    token: &str,
    property_id: &str,
) -> Result<Vec<(String, callistseo_storage::Ga4Metrics)>, String> {
    let resp = http
        .post(format!("{}/{}:runReport", ep.ga4, property_id.trim()))
        .bearer_auth(token)
        .json(&serde_json::json!({
            "dateRanges": [{"startDate": "28daysAgo", "endDate": "yesterday"}],
            "dimensions": [{"name": "pagePath"}],
            "metrics": [{"name": "sessions"}, {"name": "screenPageViews"}, {"name": "engagementRate"}],
            "limit": "250000",
        }))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(google_error(resp).await);
    }
    Ok(parse_ga4(&resp.json().await.map_err(|e| e.to_string())?))
}

// ---- compte de service : jeton OAuth (JWT signé RS256) --------------------------------------

#[derive(Deserialize)]
pub struct ServiceAccount {
    pub client_email: String,
    pub private_key: String,
    #[serde(default = "token_uri")]
    pub token_uri: String,
}

fn token_uri() -> String {
    "https://oauth2.googleapis.com/token".into()
}

impl ServiceAccount {
    pub fn parse(json: &str) -> Result<Self, String> {
        let sa: Self = serde_json::from_str(json).map_err(|_| {
            "compte de service : fichier JSON de clé Google attendu (client_email, private_key)"
                .to_owned()
        })?;
        // La clé doit être utilisable tout de suite : une erreur à l'enregistrement plutôt qu'à l'audit.
        sa.key_pair()?;
        if !sa.token_uri.starts_with("https://") {
            return Err("compte de service : token_uri doit être en https".into());
        }
        Ok(sa)
    }

    fn key_pair(&self) -> Result<ring::rsa::KeyPair, String> {
        let b64: String = self
            .private_key
            .lines()
            .filter(|l| !l.starts_with("-----"))
            .collect();
        let der = base64::engine::general_purpose::STANDARD
            .decode(b64.trim())
            .map_err(|_| {
                "compte de service : private_key illisible (PEM PKCS#8 attendu)".to_owned()
            })?;
        ring::rsa::KeyPair::from_pkcs8(&der)
            .map_err(|_| "compte de service : private_key refusée (RSA PKCS#8 attendu)".to_owned())
    }

    /// JWT signé demandant un jeton d'accès pour `scope`, valable une heure à partir de `now`.
    pub fn assertion(&self, scope: &str, now: i64) -> Result<String, String> {
        let b64 = |v: &[u8]| base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(v);
        let header = b64(br#"{"alg":"RS256","typ":"JWT"}"#);
        let claims = serde_json::json!({
            "iss": self.client_email, "scope": scope, "aud": self.token_uri,
            "iat": now, "exp": now + 3600,
        });
        let msg = format!("{header}.{}", b64(claims.to_string().as_bytes()));
        let key = self.key_pair()?;
        let mut sig = vec![0u8; key.public().modulus_len()];
        key.sign(
            &ring::signature::RSA_PKCS1_SHA256,
            &ring::rand::SystemRandom::new(),
            msg.as_bytes(),
            &mut sig,
        )
        .map_err(|_| "compte de service : signature impossible".to_owned())?;
        Ok(format!("{msg}.{}", b64(&sig)))
    }
}

// ---- dates ----------------------------------------------------------------------------------

/// Date civile `AAAA-MM-JJ` d'un nombre de jours depuis le 1970-01-01 (algorithme de H. Hinnant).
pub fn civil_date(days: i64) -> String {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn psi_prefers_page_field_data_then_lab() {
        let lab = serde_json::json!({"lighthouseResult": {
            "categories": {"performance": {"score": 0.42}},
            "audits": {"largest-contentful-paint": {"numericValue": 4123.6}, "cumulative-layout-shift": {"numericValue": 0.31}}}});
        let m = parse_psi(&lab).unwrap();
        assert_eq!(
            (m.score, m.lcp_ms, m.cls, m.inp_ms, m.source),
            (Some(42), Some(4124), Some(0.31), None, "labo")
        );

        let mut field = lab.clone();
        field["loadingExperience"] = serde_json::json!({"metrics": {
            "LARGEST_CONTENTFUL_PAINT_MS": {"percentile": 2100, "category": "FAST"},
            "CUMULATIVE_LAYOUT_SHIFT_SCORE": {"percentile": 5, "distributions": [{"min": 0, "max": 10}]},
            "INTERACTION_TO_NEXT_PAINT": {"percentile": 650}}});
        let m = parse_psi(&field).unwrap();
        assert_eq!(
            (m.lcp_ms, m.cls, m.inp_ms, m.source),
            (Some(2100), Some(0.05), Some(650), "terrain")
        );

        // Données de l'origine seulement : pas celles de la page, on garde le laboratoire.
        field["loadingExperience"]["origin_fallback"] = serde_json::json!(true);
        assert_eq!(parse_psi(&field).unwrap().source, "labo");

        let broken = serde_json::json!({"lighthouseResult": {"runtimeError": {"code": "NO_FCP", "message": "rien affiché"}}});
        assert!(parse_psi(&broken).unwrap_err().contains("rien affiché"));
    }

    #[test]
    fn gsc_and_ga4_rows() {
        let gsc = serde_json::json!({"rows": [
            {"keys": ["https://a.test/"], "clicks": 3.0, "impressions": 120.0, "ctr": 0.025, "position": 4.2},
            {"keys": []}]});
        let rows = parse_gsc(&gsc);
        assert_eq!(rows.len(), 1);
        assert_eq!(
            (rows[0].0.as_str(), rows[0].1.impressions),
            ("https://a.test/", 120)
        );
        assert!(
            parse_gsc(&serde_json::json!({})).is_empty(),
            "aucune donnée : pas de champ rows"
        );

        let ga4 = serde_json::json!({
            "metricHeaders": [{"name": "sessions"}, {"name": "screenPageViews"}, {"name": "engagementRate"}],
            "rows": [{"dimensionValues": [{"value": "/blog"}], "metricValues": [{"value": "12"}, {"value": "30"}, {"value": "0.5"}]}]});
        let rows = parse_ga4(&ga4);
        assert_eq!(rows[0].0, "/blog");
        assert_eq!(
            (rows[0].1.sessions, rows[0].1.views, rows[0].1.engagement),
            (12, 30, 0.5)
        );
    }

    #[test]
    fn civil_dates() {
        assert_eq!(civil_date(0), "1970-01-01");
        assert_eq!(civil_date(19_782), "2024-02-29");
        assert_eq!(civil_date(20_754), "2026-10-28");
        assert_eq!(civil_date(-1), "1969-12-31");
    }

    #[test]
    fn config_is_validated() {
        let ok = IntegrationsConfig {
            psi: Some(PsiConfig {
                strategy: "mobile".into(),
                max_urls: 20,
            }),
            gsc: Some(GscConfig {
                property: "sc-domain:exemple.fr".into(),
            }),
            ga4: Some(Ga4Config {
                property_id: "123456789".into(),
            }),
        };
        assert!(ok.validate().is_ok());
        for bad in [
            IntegrationsConfig {
                psi: Some(PsiConfig {
                    strategy: "tv".into(),
                    max_urls: 1,
                }),
                ..Default::default()
            },
            IntegrationsConfig {
                psi: Some(PsiConfig {
                    strategy: "mobile".into(),
                    max_urls: 500,
                }),
                ..Default::default()
            },
            IntegrationsConfig {
                gsc: Some(GscConfig {
                    property: "exemple.fr".into(),
                }),
                ..Default::default()
            },
            IntegrationsConfig {
                ga4: Some(Ga4Config {
                    property_id: "G-ABC123".into(),
                }),
                ..Default::default()
            },
        ] {
            assert!(bad.validate().is_err(), "{bad:?}");
        }
    }

    /// Clé RSA de test uniquement (générée pour ce test, sans aucun usage réel).
    const TEST_KEY: &str = include_str!("../tests/fixtures/test-service-account-key.pem");

    #[test]
    fn service_account_signs_a_verifiable_jwt() {
        let json = serde_json::json!({
            "type": "service_account", "client_email": "robot@projet.iam.gserviceaccount.com",
            "private_key": TEST_KEY, "token_uri": "https://oauth2.googleapis.com/token",
        })
        .to_string();
        let sa = ServiceAccount::parse(&json).unwrap();
        let jwt = sa
            .assertion("https://www.googleapis.com/auth/webmasters.readonly", 1_000)
            .unwrap();
        let parts: Vec<_> = jwt.split('.').collect();
        assert_eq!(parts.len(), 3);
        let dec = |s: &str| {
            base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(s)
                .unwrap()
        };
        let claims: serde_json::Value = serde_json::from_slice(&dec(parts[1])).unwrap();
        assert_eq!(claims["iss"], "robot@projet.iam.gserviceaccount.com");
        assert_eq!(
            (claims["iat"].as_i64(), claims["exp"].as_i64()),
            (Some(1_000), Some(4_600))
        );
        // La signature se vérifie avec la clé publique.
        let public = sa.key_pair().unwrap().public().as_ref().to_vec();
        ring::signature::UnparsedPublicKey::new(
            &ring::signature::RSA_PKCS1_2048_8192_SHA256,
            public,
        )
        .verify(
            format!("{}.{}", parts[0], parts[1]).as_bytes(),
            &dec(parts[2]),
        )
        .unwrap();
        // Fichiers invalides : refusés dès l'enregistrement.
        assert!(ServiceAccount::parse("{}").is_err());
        assert!(
            ServiceAccount::parse(r#"{"client_email":"a","private_key":"pas une clé"}"#).is_err()
        );
    }
}
