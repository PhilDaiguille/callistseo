use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use url::Url;

/// Normalise une URL : http/https uniquement, sans fragment.
/// Les paramètres de requête sont conservés tels quels (ils peuvent changer le contenu).
pub fn normalize(raw: &str, base: Option<&Url>) -> Option<Url> {
    let mut u = match base {
        Some(b) => b.join(raw).ok()?,
        None => Url::parse(raw).ok()?,
    };
    if !matches!(u.scheme(), "http" | "https") {
        return None;
    }
    u.set_fragment(None);
    // RFC 3986 §6.2.2.1 : `%e2` et `%E2` désignent la même URL. Sans cette étape, une même page
    // liée sous les deux écritures est explorée deux fois et signalée comme contenu dupliqué.
    if let Some(path) = uppercase_percent(u.path()) {
        u.set_path(&path);
    }
    if let Some(query) = u.query().and_then(uppercase_percent) {
        u.set_query(Some(&query));
    }
    // « /page? » et « /page » sont la même ressource.
    if u.query() == Some("") {
        u.set_query(None);
    }
    // `url` supprime déjà les ports par défaut et normalise le chemin vide en "/".
    Some(u)
}

/// Le texte avec les chiffres hexadécimaux des `%xx` en majuscules ; `None` si rien ne change.
fn uppercase_percent(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let changed = b.windows(3).any(|w| {
        w[0] == b'%'
            && (w[1].is_ascii_lowercase() || w[2].is_ascii_lowercase())
            && w[1].is_ascii_hexdigit()
            && w[2].is_ascii_hexdigit()
    });
    if !changed {
        return None;
    }
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && i + 2 < b.len()
            && b[i + 1].is_ascii_hexdigit()
            && b[i + 2].is_ascii_hexdigit()
        {
            out.push('%');
            out.push(b[i + 1].to_ascii_uppercase() as char);
            out.push(b[i + 2].to_ascii_uppercase() as char);
            i += 3;
        } else {
            // Les octets non ASCII ne peuvent pas apparaître ici : `url` les a déjà encodés.
            out.push(b[i] as char);
            i += 1;
        }
    }
    Some(out)
}

/// Identifiant par défaut : `CallistSEO/<version>`. Il ne contient volontairement aucune URL :
/// l'opérateur doit y mettre la sienne (`--user-agent "CallistSEO/0.1 (+https://…)"`), car une
/// adresse de contact fictive induirait les administrateurs en erreur.
pub fn default_user_agent() -> String {
    format!("CallistSEO/{}", env!("CARGO_PKG_VERSION"))
}

/// Périmètre du crawl, défini par projet : quelles URL suivre et comment les normaliser.
/// Les motifs sont des expressions régulières (syntaxe `regex`) cherchées dans l'URL complète.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UrlScope {
    /// Si non vide, seules les URL qui correspondent à au moins un motif sont suivies.
    pub include: Vec<String>,
    /// Les URL qui correspondent à un motif ne sont jamais suivies (l'exclusion l'emporte).
    pub exclude: Vec<String>,
    /// Paramètres de requête retirés des URL avant déduplication (`utm_*` : préfixe).
    pub strip_params: Vec<String>,
    /// URL plus longues (en caractères) : non suivies.
    pub max_url_length: Option<usize>,
    /// robots.txt à utiliser à la place de celui du site (pour tester une version à venir).
    pub robots_override: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtractKind {
    /// Expression régulière sur le HTML source : le premier groupe capturant, sinon toute la correspondance.
    Regex,
    /// Sélecteur CSS : le texte de chaque élément, ou la valeur de l'attribut demandé.
    Css,
    /// Recherche dans le code source : une seule valeur « N » (nombre d'occurrences) si le texte est trouvé.
    Contains,
    /// Expression XPath : le texte de chaque élément, la valeur d'un attribut (`//a/@href`) ou le
    /// résultat d'une fonction (`count(//img)`).
    Xpath,
}

/// Extraction personnalisée appliquée à chaque page HTML.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Extractor {
    /// Identifiant (lettres, chiffres, `_`) : sert de nom de colonne et de clé.
    pub name: String,
    pub kind: ExtractKind,
    pub pattern: String,
    /// Pour `Css` : attribut à lire (`href`, `content`…) au lieu du texte.
    pub attr: Option<String>,
}

/// Plafonds de ce qui est conservé par extracteur et par page.
pub const MAX_EXTRACTORS: usize = 10;
pub const MAX_EXTRACTED_VALUES: usize = 20;
pub const MAX_EXTRACTED_LEN: usize = 300;

/// Accès à un site protégé : envoyé seulement à l'origine exacte de l'URL de départ (même schéma,
/// hôte et port), jamais aux autres sites. Le `Debug` masque tout : rien ne part dans les journaux.
#[derive(Clone, Default, PartialEq)]
pub struct SiteAuth {
    /// Authentification HTTP Basic (utilisateur, mot de passe).
    pub basic: Option<(String, String)>,
    /// En-têtes ajoutés à chaque requête (jeton, clé d'accès à une préproduction…).
    pub headers: Vec<(String, String)>,
    /// En-tête `Cookie` (session ouverte dans un navigateur, par exemple).
    pub cookies: Option<String>,
    /// Connexion par formulaire avant l'audit : les cookies de session obtenus s'ajoutent à `cookies`.
    pub form: Option<FormLogin>,
}

/// Formulaire de connexion : page qui le contient (même origine que le site), identifiants, et
/// noms des champs si la détection automatique (champ mot de passe + champ texte précédent) échoue.
#[derive(Clone, Default, PartialEq)]
pub struct FormLogin {
    pub url: String,
    pub username: String,
    pub password: String,
    pub username_field: Option<String>,
    pub password_field: Option<String>,
}

impl std::fmt::Debug for SiteAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SiteAuth(***)")
    }
}

impl SiteAuth {
    pub fn is_empty(&self) -> bool {
        self.basic.is_none()
            && self.headers.is_empty()
            && self.cookies.is_none()
            && self.form.is_none()
    }
}

#[derive(Debug, Clone)]
pub struct CrawlConfig {
    pub max_pages: usize,
    pub max_depth: usize,
    /// Concurrence de départ (et plancher).
    pub concurrency: usize,
    /// Plafond de la concurrence adaptative (`= concurrency` : concurrence fixe).
    pub max_concurrency: usize,
    pub timeout: Duration,
    /// Délai minimal entre le début de deux requêtes (0 = aucun). Le `Crawl-delay` de robots.txt
    /// s'y ajoute : c'est la valeur la plus grande des deux qui s'applique.
    pub delay: Duration,
    pub max_body_bytes: usize,
    pub user_agent: String,
    /// Autorise les IP privées/loopback. Réservé aux tests locaux.
    pub allow_private: bool,
    pub respect_robots: bool,
    pub scope: UrlScope,
    pub extractors: Vec<Extractor>,
    /// Vérifie aussi les liens vers d'autres hôtes (requêtes vers des sites tiers : option).
    pub check_external: bool,
    /// Identifiants du site audité (jamais enregistrés avec l'audit).
    pub auth: SiteAuth,
}

impl Default for CrawlConfig {
    fn default() -> Self {
        Self {
            max_pages: 1000,
            max_depth: 10,
            concurrency: 8,
            max_concurrency: 16,
            timeout: Duration::from_secs(15),
            delay: Duration::ZERO,
            max_body_bytes: 5 * 1024 * 1024,
            user_agent: default_user_agent(),
            allow_private: false,
            respect_robots: true,
            scope: UrlScope::default(),
            extractors: Vec::new(),
            check_external: false,
            auth: SiteAuth::default(),
        }
    }
}

/// Faits observés dans le HTML (aucun jugement : les règles viennent après).
#[derive(Debug, Clone, Default)]
pub struct PageMeta {
    pub title: Option<String>,
    pub description: Option<String>,
    pub h1: Vec<String>,
    pub canonical: Option<Url>,
    /// Contenu brut de `<meta name="robots">`, en minuscules.
    pub robots: Option<String>,
}

/// En-têtes de réponse conservés (utiles aux règles : indexation, sécurité, cache). Les autres sont
/// ignorés pour borner la mémoire et la taille de la base.
pub const RECORDED_HEADERS: &[&str] = &[
    "x-robots-tag",
    "link",
    "last-modified",
    "content-language",
    "cache-control",
    "strict-transport-security",
    "content-security-policy",
    "x-content-type-options",
    "x-frame-options",
    "referrer-policy",
];

/// Plafonds par page des listes extraites (une page de presse peut avoir des milliers d'images).
pub const MAX_IMAGES: usize = 100;
pub const MAX_H2: usize = 30;
pub const MAX_JSON_LD: usize = 10;
pub const MAX_JSON_LD_BYTES: usize = 20 * 1024;

/// Faits mesurés sur la réponse et le contenu (aucun jugement : les règles viennent après).
#[derive(Debug, Clone, Default)]
pub struct PageFacts {
    /// Délai jusqu'aux en-têtes de réponse, puis jusqu'à la fin du corps (ms).
    pub ttfb_ms: Option<u32>,
    pub total_ms: Option<u32>,
    /// Taille du corps lu (octets, tronqué à `max_body_bytes`) ou `Content-Length`.
    pub size: Option<u64>,
    pub content_type: Option<String>,
    /// En-têtes de `RECORDED_HEADERS` (noms en minuscules, valeurs tronquées à 300 caractères).
    pub headers: Vec<(String, String)>,
    /// Empreinte FNV-1a 64 bits (hexadécimal) du texte visible normalisé : deux pages au même
    /// texte ont la même empreinte (doublons exacts), quel que soit le balisage.
    pub content_hash: Option<String>,
    /// Simhash 64 bits (hexadécimal) du texte visible : deux pages quasi identiques ont des
    /// simhash proches (faible distance de Hamming).
    pub simhash: Option<String>,
    /// URL du `<link rel="amphtml">` (version AMP de la page).
    pub amp_html: Option<String>,
    /// La page est elle-même AMP (`<html amp>` ou `<html ⚡>`).
    pub is_amp: bool,
    /// Éléments microdata (`itemscope`) et, parmi eux, ceux sans `itemtype`.
    pub microdata: u32,
    pub microdata_untyped: u32,
    /// `<lastmod>` de l'URL dans le sitemap, tel qu'écrit (non validé ici).
    pub sitemap_lastmod: Option<String>,
    /// Liens `<a>` sans aucun texte accessible (ni texte, ni alt d'image, ni aria-label/title).
    pub anchor_empty: u32,
    /// Liens dont le texte est générique (« cliquez ici », « read more »…).
    pub anchor_generic: u32,
    /// (URL, raison) des liens externes en échec (option `check_external`).
    pub external_broken: Vec<(String, String)>,
    /// Robots IA (GPTBot, ClaudeBot…) que le robots.txt interdit explicitement sur tout le site ; renseigné sur l'URL de départ seulement.
    pub ai_bots_blocked: Vec<String>,
    pub word_count: Option<u32>,
    /// Attribut `lang` de `<html>`.
    pub lang: Option<String>,
    pub viewport: Option<String>,
    pub h2: Vec<String>,
    /// (code de langue, URL absolue) des `<link rel="alternate" hreflang>`.
    pub hreflang: Vec<(String, String)>,
    /// (src, alt) des premières images ; `alt` absent = `None`, vide = `Some("")`.
    pub images: Vec<(String, Option<String>)>,
    /// Nombre total d'images (au-delà de `MAX_IMAGES` elles ne sont plus listées).
    pub images_total: u32,
    /// Blocs `application/ld+json` bruts (plafonnés).
    pub json_ld: Vec<String>,
    /// Liens sortants `rel="nofollow"`.
    pub nofollow_links: u32,
    /// Résultats des extracteurs personnalisés (nom, valeurs) ; un extracteur sans résultat est absent.
    pub extracted: Vec<(String, Vec<String>)>,
    /// Nombre de `<title>` (dans `<head>`), de meta description et de canonical (schéma v16).
    /// `None` : audit plus ancien, non mesuré.
    pub title_tags: Option<u32>,
    pub description_tags: Option<u32>,
    pub canonical_tags: Option<u32>,
    /// Images sans attributs `width` et `height` (risque de décalage de mise en page, CLS).
    pub images_unsized: Option<u32>,
    /// Balises de partage présentes et non vides, parmi `SOCIAL_TAGS`.
    pub social: Vec<String>,
    /// Présence d'un `<link rel="icon">`.
    pub icon: Option<bool>,
    /// Feuilles de style et scripts du même hôte que robots.txt interdit à Googlebot (schéma v17,
    /// plafonné à `MAX_RESOURCES`). Le crawler y range d'abord tous les candidats, puis filtre.
    pub blocked_resources: Vec<String>,
}

/// Plafond des ressources CSS / JS relevées par page.
pub const MAX_RESOURCES: usize = 20;

/// Balises de partage social relevées (Open Graph et carte Twitter / X).
pub const SOCIAL_TAGS: &[&str] = &["og:title", "og:description", "og:image", "twitter:card"];

impl PageFacts {
    /// Valeur d'un en-tête enregistré.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }
}

/// Table d'internement : une seule allocation par URL distincte, partagée (`Arc`) par toutes les
/// pages qui la citent. Sur une page de presse (300 liens), c'est ≈ 28× moins de mémoire.
#[derive(Default)]
pub struct UrlInterner(HashSet<Arc<Url>>);

impl UrlInterner {
    pub fn intern(&mut self, url: Url) -> Arc<Url> {
        if let Some(known) = self.0.get(&url) {
            return known.clone();
        }
        let shared = Arc::new(url);
        self.0.insert(shared.clone());
        shared
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Erreur posée par le crawler quand robots.txt interdit l'URL (ce n'est pas une panne).
pub const ROBOTS_BLOCKED: &str = "bloqué par robots.txt";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Error,
    Warning,
    Info,
}

impl Severity {
    /// Nom stocké en base (identique à `Debug`, sans allocation).
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Error => "Error",
            Severity::Warning => "Warning",
            Severity::Info => "Info",
        }
    }
}

/// Problème détecté sur une URL par une règle.
#[derive(Debug, Clone)]
pub struct Issue {
    pub rule: &'static str,
    pub severity: Severity,
    pub url: Url,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct CrawledPage {
    /// Listée dans un sitemap.
    pub in_sitemap: bool,
    /// Réponse 2xx de type HTML (les métadonnées n'ont de sens que dans ce cas).
    pub is_html: bool,
    pub meta: PageMeta,
    pub facts: PageFacts,
    pub url: Url,
    pub depth: usize,
    pub status: Option<u16>,
    pub redirect_to: Option<Url>,
    /// Liens sortants (normalisés). `Arc` partagés via `UrlInterner` : une URL citée par des
    /// centaines de pages n'est stockée qu'une fois (≈ 8 o par lien au lieu de ≈ 225 o).
    pub links: Vec<Arc<Url>>,
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    #[test]
    fn percent_encoding_case_is_normalized() {
        let n = |s: &str| normalize(s, None).unwrap().to_string();
        // Cas réel : espace fine insécable (U+2009) écrite des deux façons.
        assert_eq!(
            n("https://a.test/c/depot%e2%80%89-13105/"),
            n("https://a.test/c/depot%E2%80%89-13105/")
        );
        assert_eq!(n("https://a.test/x?q=%c3%a9"), "https://a.test/x?q=%C3%A9");
        // Rien d'autre ne bouge : déjà en majuscules, % isolé, texte ordinaire.
        assert_eq!(n("https://a.test/a%2Fb"), "https://a.test/a%2Fb");
        assert_eq!(n("https://a.test/100%"), "https://a.test/100%");
        assert_eq!(n("https://a.test/abc?x=1"), "https://a.test/abc?x=1");
        assert_eq!(n("https://a.test/abc?"), "https://a.test/abc");
    }

    use super::*;

    #[test]
    fn interner_shares_one_allocation_per_url() {
        let mut pool = UrlInterner::default();
        let a = pool.intern(Url::parse("http://a.test/x").unwrap());
        let b = pool.intern(Url::parse("http://a.test/x").unwrap());
        let c = pool.intern(Url::parse("http://a.test/y").unwrap());
        assert!(Arc::ptr_eq(&a, &b) && !Arc::ptr_eq(&a, &c));
        assert_eq!((pool.len(), Arc::strong_count(&a)), (2, 3)); // table + a + b
    }

    #[test]
    fn normalizes() {
        let base = Url::parse("http://a.test/dir/").unwrap();
        assert_eq!(
            normalize("../x?q=1#frag", Some(&base)).unwrap().as_str(),
            "http://a.test/x?q=1"
        );
        assert_eq!(
            normalize("http://a.test:80", None).unwrap().as_str(),
            "http://a.test/"
        );
        assert!(normalize("mailto:a@b.c", None).is_none());
        assert!(normalize("javascript:void(0)", Some(&base)).is_none());
    }
}
