//! Table des règles : gravité, libellé et explication. C'est la SEULE source de la gravité
//! (`analyze` la lit ici), pour qu'elle reste cohérente d'une règle à l'autre.
//!
//! Politique :
//! - **Erreur** : quelque chose est cassé, de façon objective (page ou lien inaccessible, boucle).
//!   C'est ce qui doit faire échouer une CI.
//! - **Avertissement** : problème d'optimisation ou d'indexation probable, à corriger ; rien n'est cassé.
//! - **Info** : constat neutre ou éditorial, à vérifier.
//!
//! Ce sont des constats techniques, pas des pénalités certaines de Google.

use callistseo_core::Severity;

#[derive(Debug, Clone, Copy)]
pub struct RuleInfo {
    pub id: &'static str,
    pub severity: Severity,
    pub label: &'static str,
    pub help: &'static str,
}

const fn r(
    id: &'static str,
    severity: Severity,
    label: &'static str,
    help: &'static str,
) -> RuleInfo {
    RuleInfo {
        id,
        severity,
        label,
        help,
    }
}

pub const RULES: &[RuleInfo] = &[
    r("network-error", Severity::Error, "Erreur réseau", "La page n'a pas pu être récupérée (DNS, connexion, délai dépassé ou destination interdite)."),
    r("http-error", Severity::Error, "Erreur HTTP", "Le serveur répond avec un statut 4xx ou 5xx : la page est inaccessible aux visiteurs et aux robots."),
    r("redirect", Severity::Info, "Redirection", "L'URL redirige vers une autre. Normal en soi ; les liens internes devraient viser la destination finale."),
    r("redirect-chain", Severity::Warning, "Chaîne de redirections", "Plusieurs redirections successives avant la page finale : cela ralentit l'accès et dilue les signaux."),
    r("redirect-loop", Severity::Error, "Boucle de redirections", "Les redirections se renvoient entre elles sans jamais aboutir : la page est inaccessible."),
    r("deep-page", Severity::Info, "Page profonde", "La page est loin de l'accueil en nombre de clics. Seuil éditorial configurable."),
    r("title-missing", Severity::Warning, "Title absent", "Aucune balise <title> : les moteurs devront en deviner un."),
    r("title-empty", Severity::Warning, "Title vide", "La balise <title> existe mais ne contient rien."),
    r("title-duplicate", Severity::Warning, "Title dupliqué", "Plusieurs pages indexables partagent le même title, ce qui les rend difficiles à distinguer."),
    r("description-missing", Severity::Warning, "Description absente", "Pas de meta description : le résumé affiché dans les résultats sera choisi automatiquement."),
    r("description-empty", Severity::Warning, "Description vide", "La meta description existe mais est vide."),
    r("description-duplicate", Severity::Info, "Description dupliquée", "Plusieurs pages indexables partagent la même meta description."),
    r("h1-missing", Severity::Warning, "H1 absent", "Aucun titre de niveau 1 sur la page."),
    r("h1-multiple", Severity::Info, "H1 multiples", "Plusieurs H1 sur la page. Autorisé en HTML5, mais à vérifier selon la structure voulue."),
    r("noindex", Severity::Info, "Noindex", "La page demande à ne pas être indexée (meta robots). Vérifiez que c'est voulu."),
    r("noindex-canonical", Severity::Warning, "Noindex et canonical", "La page est en noindex tout en désignant une autre URL canonique : directives potentiellement contradictoires."),
    r("canonical-target", Severity::Warning, "Canonical problématique", "La canonical pointe vers une URL redirigée ou en erreur."),
    r("broken-link", Severity::Error, "Lien interne cassé", "Cette page contient un lien vers une URL qui répond en erreur (4xx/5xx). Un délai dépassé n'est pas compté ici : il est signalé sur la page elle-même (Erreur réseau)."),
    r("query-explosion", Severity::Warning, "Explosion de paramètres", "Un même chemin existe en très nombreuses variantes de paramètres : risque de gaspillage de crawl."),
    r("sitemap-url-error", Severity::Error, "Sitemap : URL inaccessible", "Une URL listée dans le sitemap est en erreur."),
    r("sitemap-url-redirect", Severity::Warning, "Sitemap : URL redirigée", "Une URL listée dans le sitemap redirige : le sitemap devrait contenir les URL finales."),
    r("sitemap-url-noindex", Severity::Warning, "Sitemap : URL en noindex", "Une URL du sitemap demande à ne pas être indexée : signal contradictoire."),
    r("orphan-page", Severity::Warning, "Page orpheline", "Listée au sitemap mais aucune page crawlée n'y mène par un lien."),
    r("not-in-sitemap", Severity::Info, "Absente du sitemap", "Page indexable trouvée par le crawl mais absente du sitemap."),
    r("noindex-header", Severity::Info, "Noindex (en-tête)", "Le serveur envoie X-Robots-Tag: noindex : la page demande à ne pas être indexée. Vérifiez que c'est voulu."),
    r("title-too-long", Severity::Info, "Title trop long", "Title de plus de 60 caractères : il risque d'être tronqué dans les résultats. Seuil configurable (approximation en caractères)."),
    r("title-too-short", Severity::Info, "Title trop court", "Title de moins de 30 caractères : il décrit peu la page. Seuil configurable."),
    r("description-too-long", Severity::Info, "Description trop longue", "Meta description de plus de 160 caractères : elle risque d'être tronquée. Seuil configurable."),
    r("description-too-short", Severity::Info, "Description trop courte", "Meta description de moins de 70 caractères. Seuil configurable."),
    r("h2-missing", Severity::Info, "H2 absent", "Page de contenu sans sous-titre H2 : la structure du texte est peu lisible."),
    r("thin-content", Severity::Info, "Contenu peu fourni", "Moins de 200 mots de texte visible. Seuil éditorial configurable."),
    r("content-duplicate", Severity::Warning, "Contenu dupliqué", "Plusieurs pages indexables ont exactement le même texte visible : les moteurs devront choisir laquelle indexer."),
    r("content-near-duplicate", Severity::Info, "Contenu quasi dupliqué", "Plusieurs pages indexables ont un texte presque identique (simhash, au plus 3 bits d'écart). Vérifiez s'il faut les fusionner ou leur donner une canonical."),
    r("hreflang-invalid-code", Severity::Warning, "hreflang : code invalide", "Le code de langue/région d'un hreflang n'est pas valide (attendu : fr, fr-FR, x-default…) ; il sera ignoré."),
    r("hreflang-no-self", Severity::Warning, "hreflang : pas d'auto-référence", "La page déclare des hreflang mais ne se liste pas elle-même."),
    r("hreflang-missing-return", Severity::Warning, "hreflang : lien retour manquant", "La page cible un hreflang qui ne renvoie pas vers elle : Google ignore la paire."),
    r("hreflang-target-error", Severity::Warning, "hreflang : cible problématique", "Un hreflang pointe vers une URL redirigée, en erreur ou en noindex."),
    r("insecure-http", Severity::Warning, "Page en HTTP", "La page est servie sans chiffrement (HTTP)."),
    r("hsts-missing", Severity::Info, "HSTS absent", "Pas d'en-tête Strict-Transport-Security : le navigateur n'est pas forcé à rester en HTTPS."),
    r("nosniff-missing", Severity::Info, "X-Content-Type-Options absent", "Pas d'en-tête X-Content-Type-Options: nosniff (durcissement de sécurité)."),
    r("mixed-content", Severity::Warning, "Contenu mixte", "Une page HTTPS charge des images en HTTP : les navigateurs peuvent les bloquer. Seules les images sont examinées."),
    r("slow-response", Severity::Info, "Réponse lente", "Le serveur met plus d'une seconde à répondre (premiers octets). Seuil configurable."),
    r("lang-missing", Severity::Warning, "Attribut lang absent", "La balise <html> n'a pas d'attribut lang (accessibilité et langue de la page)."),
    r("viewport-missing", Severity::Warning, "Meta viewport absente", "Sans meta viewport, l'affichage sur mobile est dégradé."),
    r("image-alt-missing", Severity::Warning, "Image sans alt", "Des images n'ont pas d'attribut alt (un alt vide reste valide pour une image décorative)."),
    r("json-ld-invalid", Severity::Warning, "JSON-LD invalide", "Un bloc de données structurées JSON-LD n'est pas du JSON valide (ou a été tronqué à 20 Ko) : il sera ignoré."),
    r("json-ld-missing-property", Severity::Warning, "JSON-LD : propriété requise absente", "Un type schema.org courant (Article, Product, Event…) n'a pas toutes les propriétés requises pour les résultats enrichis."),
    r("amp-target-error", Severity::Warning, "AMP : version problématique", "Le lien amphtml pointe vers une page redirigée, en erreur ou en noindex."),
    r("amp-no-canonical", Severity::Warning, "AMP : canonical absente", "Une page AMP doit désigner sa version classique par une balise canonical (ou elle-même si elle est autonome)."),
    r("microdata-no-type", Severity::Warning, "Microdata sans itemtype", "Un élément itemscope n'a pas d'itemtype : les moteurs ne savent pas quel type de données il décrit."),
    r("sitemap-lastmod-invalid", Severity::Warning, "Sitemap : lastmod invalide", "Le <lastmod> de cette URL n'est pas une date W3C (AAAA-MM-JJ ou date-heure avec fuseau) : les moteurs l'ignorent."),
    r("anchor-empty", Severity::Warning, "Ancre vide", "Un lien n'a aucun texte accessible (ni texte, ni alt d'image, ni aria-label) : les moteurs et les lecteurs d'écran ne savent pas où il mène."),
    r("anchor-generic", Severity::Info, "Ancre générique", "Un lien a un texte comme « cliquez ici » ou « en savoir plus » : il ne décrit pas la page visée."),
    r("external-link-broken", Severity::Warning, "Lien externe cassé", "Un lien vers un autre site répond en erreur (4xx/5xx) ou est injoignable. Vérifié seulement avec l'option de vérification des liens externes ; les 401, 403 et 429 ne sont pas comptés."),
    r("ai-snippet-blocked", Severity::Info, "IA : extrait bloqué", "Page indexable avec nosnippet ou max-snippet:0 : d'après le guide Google sur l'IA générative, une page doit pouvoir s'afficher avec un extrait pour servir de source dans Aperçu IA et le Mode IA. Vérifiez que c'est voulu."),
    r("schema-faq-howto-retired", Severity::Info, "Schéma FAQ/HowTo sans effet", "Google a retiré les résultats enrichis FAQ et HowTo (2026) : le balisage reste valide mais n'a plus d'effet visible dans la recherche."),
    r("ai-crawler-blocked", Severity::Info, "Robots d'IA tiers interdits", "Constat, pas un défaut : le robots.txt interdit tout le site à des robots d'autres services (GPTBot, ClaudeBot, PerplexityBot, CCBot, Applebot-Extended, Google-Extended). Sans effet sur la Recherche Google ni sur Aperçu IA, qui passent par Googlebot. À revoir seulement si vous voulez que ces services utilisent votre contenu."),
    r("article-author-missing", Severity::Info, "Article sans auteur", "Le JSON-LD Article n'indique pas d'auteur (author), une propriété recommandée par Google pour les résultats enrichis. Aucun balisage spécial n'est nécessaire pour les fonctionnalités d'IA."),
    r("cwv-lcp-poor", Severity::Warning, "LCP mauvais", "Core Web Vitals : le plus grand élément visible s'affiche en plus de 4 s (données réelles des visiteurs Chrome quand Google en a, sinon mesure de laboratoire PageSpeed). Google le prend en compte dans l'expérience de page."),
    r("cwv-cls-poor", Severity::Warning, "CLS mauvais", "Core Web Vitals : la mise en page bouge pendant le chargement (CLS au-delà de 0,25) ; images sans dimensions, publicités ou polices en cause le plus souvent."),
    r("cwv-inp-poor", Severity::Warning, "INP mauvais", "Core Web Vitals : la page réagit en plus de 500 ms aux interactions des visiteurs (données réelles Chrome seulement)."),
    r("psi-score-low", Severity::Info, "Score PageSpeed faible", "Score de performance Lighthouse (laboratoire, mobile par défaut) sous 50/100. Indicatif : ce sont les Core Web Vitals réels qui comptent pour Google."),
    r("gsc-no-impressions", Severity::Info, "Aucune impression Google", "Page indexable sans aucune impression dans Search Console sur 28 jours : non indexée, cannibalisée ou sans demande. À vérifier dans l'outil d'inspection d'URL."),
    r("article-date-missing", Severity::Info, "Article sans date", "Le JSON-LD Article n'a ni datePublished ni dateModified, des propriétés recommandées par Google pour les résultats enrichis. Aucun balisage spécial n'est nécessaire pour les fonctionnalités d'IA."),
    r("redirect-temporary", Severity::Warning, "Redirection temporaire", "L'URL redirige en 302, 303 ou 307 : une redirection temporaire indique que l'ancienne URL reviendra. Pour un déplacement définitif, utilisez une 301 ou une 308."),
    r("link-to-redirect", Severity::Info, "Liens vers des redirections", "Cette page fait des liens internes vers des URL qui redirigent : faites-les pointer directement vers la destination finale."),
    r("canonical-missing", Severity::Info, "Canonical absente", "Page indexable sans balise canonical : en cas de variantes (paramètres, majuscules, slash final), le moteur choisira seul l'URL de référence."),
    r("dead-end-page", Severity::Info, "Page sans lien interne", "Page indexable qui ne fait aucun lien vers une autre page du site : la navigation et le maillage s'arrêtent ici."),
    r("h1-duplicate", Severity::Info, "H1 dupliqué", "Plusieurs pages indexables ont le même H1, ce qui les rend difficiles à distinguer."),
    r("url-too-long", Severity::Info, "URL trop longue", "URL plus longue que le seuil conseillé : difficile à lire et à partager, et souvent tronquée dans les résultats. Seuil configurable."),
    r("url-uppercase", Severity::Info, "Majuscules dans l'URL", "Le chemin contient des majuscules : la plupart des serveurs distinguent /Page et /page, ce qui crée facilement des doublons."),
    r("url-underscore", Severity::Info, "Underscore dans l'URL", "Le chemin contient des « _ » : Google recommande les tirets pour séparer les mots d'une URL."),
    r("hreflang-x-default-missing", Severity::Info, "hreflang sans x-default", "La page déclare des versions linguistiques mais pas de x-default : rien n'indique quelle version montrer aux autres langues."),
    r("sitemap-url-blocked", Severity::Warning, "Sitemap : URL bloquée par robots.txt", "Une URL listée dans le sitemap est interdite par robots.txt : signal contradictoire, le moteur ne pourra pas l'explorer."),
    r("html-too-large", Severity::Warning, "HTML trop lourd", "Le HTML dépasse 2 Mo (non compressé) : Googlebot n'en lit que les 2 premiers Mo, la suite n'est pas indexée."),
    r("head-tags-duplicate", Severity::Warning, "Balises en double", "La page contient plusieurs <title>, meta description ou canonical : le moteur peut en choisir une au hasard, ou ignorer des canonicals contradictoires."),
    r("open-graph-missing", Severity::Info, "Partage social incomplet", "Il manque og:title ou og:image : un lien partagé sur les réseaux sociaux ou les messageries s'affichera sans titre ou sans image choisis."),
    r("image-size-missing", Severity::Info, "Images sans dimensions", "Des images n'ont pas d'attributs width et height : la mise en page peut bouger pendant le chargement (CLS). À vérifier si la taille est fixée en CSS."),
    r("nofollow-page", Severity::Info, "Page en nofollow", "La page demande aux moteurs de ne suivre aucun de ses liens (meta robots ou X-Robots-Tag). Vérifiez que c'est voulu."),
    r("favicon-missing", Severity::Info, "Icône du site non déclarée", "L'accueil ne déclare pas d'icône (<link rel=\"icon\">) : Google l'affiche à côté du site dans les résultats."),
    r("resource-blocked", Severity::Warning, "CSS ou JS bloqué pour Googlebot", "Le robots.txt interdit à Googlebot des feuilles de style ou des scripts du site : il ne peut pas afficher la page comme un visiteur, ce qui peut gêner la compréhension de son contenu et de son affichage mobile."),
    r("soft-404", Severity::Warning, "Fausse page 404", "La page répond 200 mais son titre annonce une page introuvable : elle devrait renvoyer un vrai statut 404 ou 410. Détection heuristique, à confirmer."),
];

static BY_ID: std::sync::LazyLock<std::collections::HashMap<&'static str, &'static RuleInfo>> =
    std::sync::LazyLock::new(|| RULES.iter().map(|r| (r.id, r)).collect());

pub fn rule_info(id: &str) -> Option<&'static RuleInfo> {
    BY_ID.get(id).copied()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn ids_are_unique_and_described() {
        let ids: HashSet<_> = RULES.iter().map(|r| r.id).collect();
        assert_eq!(ids.len(), RULES.len());
        assert!(RULES
            .iter()
            .all(|r| !r.label.is_empty() && !r.help.is_empty()));
        assert_eq!(rule_info("http-error").unwrap().label, "Erreur HTTP");
        assert!(rule_info("inconnue").is_none());
    }

    /// Politique de gravité : une Erreur veut dire « cassé », et fait échouer une CI. Ajouter une
    /// règle en Erreur doit être un choix assumé : il faut alors modifier cette liste.
    #[test]
    fn only_broken_things_are_errors() {
        let errors: Vec<_> = RULES
            .iter()
            .filter(|r| r.severity == Severity::Error)
            .map(|r| r.id)
            .collect();
        assert_eq!(
            errors,
            [
                "network-error",
                "http-error",
                "redirect-loop",
                "broken-link",
                "sitemap-url-error"
            ]
        );
        // Les manques d'optimisation (title, description, H1) ne sont jamais des erreurs.
        for id in [
            "title-missing",
            "title-empty",
            "description-missing",
            "h1-missing",
        ] {
            assert_eq!(rule_info(id).unwrap().severity, Severity::Warning, "{id}");
        }
        for id in [
            "redirect",
            "noindex",
            "h1-multiple",
            "deep-page",
            "not-in-sitemap",
            "description-duplicate",
        ] {
            assert_eq!(rule_info(id).unwrap().severity, Severity::Info, "{id}");
        }
    }

    /// Chaque règle émise par `analyze` (appels `add("…", …)`) doit être décrite ici.
    #[test]
    fn every_emitted_rule_has_info() {
        let src = concat!(
            include_str!("lib.rs"),
            include_str!("extra.rs"),
            include_str!("external.rs")
        );
        // Émises via un tableau ou une variable.
        let mut emitted: Vec<&str> = vec![
            "title-duplicate",
            "description-duplicate",
            "h1-duplicate",
            "redirect-temporary",
        ];
        for (i, _) in src.match_indices("add(") {
            let rest = src[i + 4..].trim_start();
            if let Some(rest) = rest.strip_prefix('"') {
                emitted.push(rest.split('"').next().unwrap());
            }
        }
        assert!(emitted.len() > 20, "extraction cassée : {emitted:?}");
        for id in emitted {
            assert!(rule_info(id).is_some(), "règle sans description : {id}");
        }
    }
}
