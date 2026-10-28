use callistseo_core::{CrawlConfig, SiteAuth};
use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use reqwest::redirect::Policy;
use std::error::Error;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;
use url::{Host, Url};

/// Adresses vers lesquelles un crawler ne doit jamais se connecter (SSRF).
pub fn is_forbidden(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v) => {
            let o = v.octets();
            v.is_loopback()
                || v.is_private()
                || v.is_link_local()
                || v.is_multicast()
                || o[0] == 0 // 0.0.0.0/8
                || (o[0] == 100 && (o[1] & 0xc0) == 64) // CGNAT 100.64/10
                || (o[0] == 192 && o[1] == 0 && o[2] == 0) // 192.0.0.0/24
                || (o[0] == 198 && (o[1] & 0xfe) == 18) // benchmark 198.18/15
                || o[0] >= 240 // réservé + broadcast
        }
        IpAddr::V6(v) => {
            if let Some(m) = v.to_ipv4_mapped() {
                return is_forbidden(IpAddr::V4(m));
            }
            let s = v.segments();
            v.is_loopback()
                || v.is_unspecified()
                || v.is_multicast()
                || (s[0] & 0xfe00) == 0xfc00 // unique local
                || (s[0] & 0xffc0) == 0xfe80 // link-local
                || (s[0] == 0x64 && s[1] == 0xff9b) // NAT64 : peut viser de l'IPv4 interne
        }
    }
}

/// Résolveur DNS qui filtre les IP interdites. reqwest se connecte uniquement aux
/// adresses renvoyées ici : pas de fenêtre de DNS rebinding entre vérification et connexion.
struct SafeResolver;

impl Resolve for SafeResolver {
    fn resolve(&self, name: Name) -> Resolving {
        Box::pin(async move {
            let addrs: Vec<SocketAddr> = tokio::net::lookup_host((name.as_str(), 0))
                .await?
                .filter(|a| !is_forbidden(a.ip()))
                .collect();
            if addrs.is_empty() {
                return Err("destination interdite (IP privée ou réservée)".into());
            }
            Ok(Box::new(addrs.into_iter()) as Addrs)
        })
    }
}

pub struct Fetched {
    pub status: u16,
    pub location: Option<String>,
    /// `Retry-After` (secondes ou date HTTP).
    pub retry_after: Option<Duration>,
    /// Corps HTML (tronqué à `max_body_bytes`) ; `None` si la réponse n'est pas du HTML 2xx.
    pub html: Option<Vec<u8>>,
    /// Délai jusqu'aux en-têtes, puis jusqu'à la fin du corps.
    pub ttfb: Duration,
    pub total: Duration,
    /// Octets du corps lu (HTML) ou `Content-Length` annoncé.
    pub size: Option<u64>,
    pub content_type: Option<String>,
    /// En-têtes de `RECORDED_HEADERS`.
    pub headers: Vec<(String, String)>,
}

pub struct Client {
    http: reqwest::Client,
    max_body: usize,
    allow_private: bool,
    /// Identifiants et la seule origine qui les reçoit (voir `for_site`).
    auth: Option<(url::Origin, SiteAuth)>,
}

/// En-têtes que l'accès à un site ne peut pas remplacer : protocole, identifiant du robot (fixé
/// par l'opérateur) et ceux qui ont leur propre champ (`Cookie`).
const RESERVED_HEADERS: &[&str] = &[
    "host",
    "content-length",
    "transfer-encoding",
    "connection",
    "user-agent",
    "cookie",
    "accept-encoding",
];

/// Refuse des identifiants qui donneraient une requête invalide ou contourneraient une limite.
pub fn validate_auth(a: &SiteAuth) -> Result<(), String> {
    use reqwest::header::{HeaderName, HeaderValue};
    const MAX_VALUE: usize = 8 * 1024;
    let printable = |v: &str| v.chars().all(|c| c == ' ' || c.is_ascii_graphic());
    if let Some((user, pass)) = &a.basic {
        if user.is_empty() || user.contains(':') || !printable(user) || !printable(pass) {
            return Err("authentification Basic : utilisateur non vide, sans « : », caractères ASCII imprimables".into());
        }
        if user.len() + pass.len() > MAX_VALUE {
            return Err("authentification Basic trop longue".into());
        }
    }
    if a.headers.len() > 20 {
        return Err("20 en-têtes au plus".into());
    }
    for (name, value) in &a.headers {
        let lower = name.to_ascii_lowercase();
        if HeaderName::from_bytes(name.as_bytes()).is_err() {
            return Err(format!("nom d'en-tête invalide : {name}"));
        }
        if RESERVED_HEADERS.contains(&lower.as_str()) {
            return Err(format!("en-tête réservé : {name}"));
        }
        if lower == "authorization" && a.basic.is_some() {
            return Err("Authorization et authentification Basic : choisir l'un des deux".into());
        }
        if value.len() > MAX_VALUE || HeaderValue::from_str(value).is_err() || !printable(value) {
            return Err(format!("valeur d'en-tête invalide pour {name}"));
        }
    }
    if let Some(c) = &a.cookies {
        if c.is_empty() || c.len() > MAX_VALUE || !printable(c) {
            return Err(
                "cookies : 1 à 8 192 caractères ASCII imprimables (nom=valeur; nom2=valeur2)"
                    .into(),
            );
        }
    }
    Ok(())
}

/// Un user-agent vide, trop long ou avec des caractères de contrôle/non ASCII est refusé
/// (en-tête HTTP invalide, ou identifiant inexploitable pour un administrateur).
pub fn validate_user_agent(ua: &str) -> Result<(), String> {
    if ua.trim().is_empty()
        || ua.len() > 200
        || !ua.chars().all(|c| c == ' ' || c.is_ascii_graphic())
    {
        return Err("user-agent invalide : 1 à 200 caractères ASCII imprimables".into());
    }
    Ok(())
}

impl Client {
    pub fn new(cfg: &CrawlConfig) -> Result<Self, String> {
        validate_user_agent(&cfg.user_agent)?;
        // Redirections gérées par le crawler (chaque saut est une URL revalidée) ;
        // `no_proxy` pour qu'un proxy système ne contourne pas le filtre DNS.
        let mut b = reqwest::Client::builder()
            .redirect(Policy::none())
            .no_proxy()
            .timeout(cfg.timeout)
            .user_agent(&cfg.user_agent);
        if !cfg.allow_private {
            b = b.dns_resolver(Arc::new(SafeResolver));
        }
        Ok(Self {
            http: b.build().map_err(|e| e.to_string())?,
            max_body: cfg.max_body_bytes,
            allow_private: cfg.allow_private,
            auth: None,
        })
    }

    /// Client d'un audit : les identifiants de `cfg.auth` ne partent que vers l'origine de `start`
    /// (même schéma, hôte et port). Ni les autres sites, ni la version HTTP d'un site HTTPS.
    pub fn for_site(cfg: &CrawlConfig, start: &Url) -> Result<Self, String> {
        let mut c = Self::new(cfg)?;
        if !cfg.auth.is_empty() {
            validate_auth(&cfg.auth)?;
            c.auth = Some((start.origin(), cfg.auth.clone()));
        }
        Ok(c)
    }

    /// Requête GET, avec les identifiants si `url` est l'origine de l'audit.
    fn get(&self, url: &Url) -> reqwest::RequestBuilder {
        let mut req = self.http.get(url.clone());
        if let Some((origin, auth)) = &self.auth {
            if url.origin() == *origin {
                if let Some((user, pass)) = &auth.basic {
                    req = req.basic_auth(user, Some(pass));
                }
                for (name, value) in &auth.headers {
                    req = req.header(name, value);
                }
                if let Some(c) = &auth.cookies {
                    req = req.header(reqwest::header::COOKIE, c);
                }
            }
        }
        req
    }

    /// Les IP littérales ne passent pas par le résolveur : on les vérifie ici.
    fn check(&self, url: &Url) -> Result<(), String> {
        if self.allow_private {
            return Ok(());
        }
        let ip = match url.host() {
            Some(Host::Ipv4(ip)) => Some(IpAddr::V4(ip)),
            Some(Host::Ipv6(ip)) => Some(IpAddr::V6(ip)),
            _ => None,
        };
        if ip.is_some_and(is_forbidden) {
            return Err("destination interdite (IP privée ou réservée)".into());
        }
        Ok(())
    }

    /// robots.txt de l'origine ; `None` si absent (4xx) ou injoignable (= tout autorisé).
    /// Un 5xx vaut « tout interdit » (comportement de Google) : `--ignore-robots` le contourne.
    pub async fn robots_txt(&self, origin: &Url) -> Option<String> {
        match self
            .get_text_status(&origin.join("/robots.txt").ok()?, 512 * 1024)
            .await
        {
            Ok(t) => Some(t),
            Err(Some(s)) if s >= 500 => Some("User-agent: *\nDisallow: /\n".into()),
            Err(_) => None,
        }
    }

    /// Corps texte d'une réponse 2xx (tronqué à `max` octets) ; `None` sinon.
    pub async fn get_text(&self, url: &Url, max: usize) -> Option<String> {
        self.get_text_status(url, max).await.ok()
    }

    /// Comme `get_text`, mais l'erreur porte le statut HTTP (`None` = échec réseau ou URL interdite).
    async fn get_text_status(&self, url: &Url, max: usize) -> Result<String, Option<u16>> {
        self.check(url).map_err(|_| None)?;
        let mut resp = self.get(url).send().await.map_err(|_| None)?;
        if !resp.status().is_success() {
            return Err(Some(resp.status().as_u16()));
        }
        let mut buf = Vec::new();
        while let Some(c) = resp.chunk().await.map_err(|_| None)? {
            buf.extend_from_slice(&c);
            if buf.len() >= max {
                buf.truncate(max);
                break;
            }
        }
        Ok(String::from_utf8_lossy(&gunzip_if_needed(buf, max)).into_owned())
    }

    pub async fn fetch(&self, url: &Url) -> Result<Fetched, String> {
        self.check(url)?;
        let started = std::time::Instant::now();
        let mut resp = self.get(url).send().await.map_err(chain)?;
        let ttfb = started.elapsed();
        let status = resp.status().as_u16();
        let headers: Vec<(String, String)> = callistseo_core::RECORDED_HEADERS
            .iter()
            .filter_map(|name| {
                let v = resp.headers().get(*name)?.to_str().ok()?;
                Some(((*name).to_owned(), v.chars().take(300).collect()))
            })
            .collect();
        let content_type = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|v| v.chars().take(100).collect::<String>());
        let content_length = resp.content_length();
        let location = resp
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        let retry_after = resp
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(retry_after_secs)
            .map(Duration::from_secs);
        let is_html = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.contains("html"));
        let html = if is_html && resp.status().is_success() {
            let mut buf = Vec::new();
            while let Some(chunk) = resp.chunk().await.map_err(chain)? {
                buf.extend_from_slice(&chunk);
                if buf.len() >= self.max_body {
                    buf.truncate(self.max_body);
                    break;
                }
            }
            Some(buf)
        } else {
            None
        };
        let size = html.as_ref().map(|b| b.len() as u64).or(content_length);
        Ok(Fetched {
            status,
            location,
            retry_after,
            html,
            ttfb,
            total: started.elapsed(),
            size,
            content_type,
            headers,
        })
    }
}

/// Décompresse un corps gzip (détecté par ses octets magiques, ex. `sitemap.xml.gz`).
/// La sortie est plafonnée à `max` octets : une « bombe » gzip ne peut pas saturer la mémoire.
fn gunzip_if_needed(buf: Vec<u8>, max: usize) -> Vec<u8> {
    use std::io::Read;
    if !buf.starts_with(&[0x1f, 0x8b]) {
        return buf;
    }
    let mut out = Vec::new();
    // Une archive corrompue renvoie ce qui a été lu avant l'erreur.
    let _ = flate2::read::GzDecoder::new(&buf[..])
        .take(max as u64)
        .read_to_end(&mut out);
    out
}

fn chain(e: reqwest::Error) -> String {
    let mut msg = e.to_string();
    let mut src = e.source();
    while let Some(s) = src {
        msg.push_str(": ");
        msg.push_str(&s.to_string());
        src = s.source();
    }
    msg
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_agent_validation() {
        assert!(validate_user_agent("CallistSEO/0.1 (+https://exemple.fr/bot)").is_ok());
        for bad in ["", "   ", "é", "a\r\nX-Evil: 1", "a\tb", &"x".repeat(201)] {
            assert!(validate_user_agent(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn gunzip_roundtrip_plain_and_bomb() {
        use flate2::{write::GzEncoder, Compression};
        use std::io::Write;
        let gz = |data: &[u8]| {
            let mut e = GzEncoder::new(Vec::new(), Compression::default());
            e.write_all(data).unwrap();
            e.finish().unwrap()
        };
        assert_eq!(gunzip_if_needed(gz(b"<urlset/>"), 1000), b"<urlset/>");
        assert_eq!(gunzip_if_needed(b"<urlset/>".to_vec(), 1000), b"<urlset/>");
        let bomb = gz(&vec![b'a'; 5_000_000]);
        assert!(bomb.len() < 10_000);
        assert_eq!(gunzip_if_needed(bomb, 1000).len(), 1000);
    }

    #[test]
    fn forbidden_ips() {
        for ip in [
            "127.0.0.1",
            "10.0.0.1",
            "172.16.0.1",
            "192.168.1.1",
            "169.254.169.254",
            "100.64.0.1",
            "0.0.0.0",
            "255.255.255.255",
            "::1",
            "fc00::1",
            "fe80::1",
            "::ffff:127.0.0.1",
            "64:ff9b::7f00:1",
        ] {
            assert!(is_forbidden(ip.parse().unwrap()), "{ip}");
        }
        for ip in ["93.184.216.34", "8.8.8.8", "2606:4700::1111"] {
            assert!(!is_forbidden(ip.parse().unwrap()), "{ip}");
        }
    }
}

/// `Retry-After` : nombre de secondes, ou date HTTP (dans le passé = 0).
fn retry_after_secs(v: &str) -> Option<u64> {
    let v = v.trim();
    v.parse().ok().or_else(|| {
        let at = httpdate::parse_http_date(v).ok()?;
        Some(
            at.duration_since(std::time::SystemTime::now())
                .map_or(0, |d| d.as_secs()),
        )
    })
}

#[cfg(test)]
mod retry_after_tests {
    use super::retry_after_secs;

    #[test]
    fn seconds_and_http_dates() {
        assert_eq!(retry_after_secs(" 7 "), Some(7));
        assert_eq!(retry_after_secs("Wed, 21 Oct 2015 07:28:00 GMT"), Some(0));
        assert!(retry_after_secs("Fri, 31 Dec 2100 23:59:59 GMT").unwrap() > 1_000_000);
        assert_eq!(retry_after_secs("bientôt"), None);
    }
}
