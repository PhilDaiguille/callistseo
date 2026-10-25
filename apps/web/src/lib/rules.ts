import type { Severity } from './api'

export interface RuleInfo {
  label: string
  help: string
}

// Explication de chaque règle : ce qui a été observé, et pourquoi cela peut compter.
// Ce sont des constats techniques, pas des pénalités certaines de Google.
export const RULES: Record<string, RuleInfo> = {
  'network-error': { label: 'Erreur réseau', help: "La page n'a pas pu être récupérée (DNS, connexion, délai dépassé ou destination interdite)." },
  'http-error': { label: 'Erreur HTTP', help: 'Le serveur répond avec un statut 4xx ou 5xx : la page est inaccessible aux visiteurs et aux robots.' },
  redirect: { label: 'Redirection', help: "L'URL redirige vers une autre. Normal en soi ; les liens internes devraient viser la destination finale." },
  'redirect-chain': { label: 'Chaîne de redirections', help: 'Plusieurs redirections successives avant la page finale : cela ralentit l’accès et dilue les signaux.' },
  'redirect-loop': { label: 'Boucle de redirections', help: 'Les redirections se renvoient entre elles sans jamais aboutir : la page est inaccessible.' },
  'deep-page': { label: 'Page profonde', help: 'La page est loin de l’accueil en nombre de clics. Seuil éditorial configurable.' },
  'title-missing': { label: 'Title absent', help: 'Aucune balise <title> : les moteurs devront en deviner un.' },
  'title-empty': { label: 'Title vide', help: 'La balise <title> existe mais ne contient rien.' },
  'title-duplicate': { label: 'Title dupliqué', help: 'Plusieurs pages indexables partagent le même title, ce qui les rend difficiles à distinguer.' },
  'description-missing': { label: 'Description absente', help: 'Pas de meta description : le résumé affiché dans les résultats sera choisi automatiquement.' },
  'description-empty': { label: 'Description vide', help: 'La meta description existe mais est vide.' },
  'description-duplicate': { label: 'Description dupliquée', help: 'Plusieurs pages indexables partagent la même meta description.' },
  'h1-missing': { label: 'H1 absent', help: 'Aucun titre de niveau 1 sur la page.' },
  'h1-multiple': { label: 'H1 multiples', help: 'Plusieurs H1 sur la page. Autorisé en HTML5, mais à vérifier selon la structure voulue.' },
  noindex: { label: 'Noindex', help: 'La page demande à ne pas être indexée (meta robots). Vérifiez que c’est voulu.' },
  'noindex-canonical': { label: 'Noindex et canonical', help: 'La page est en noindex tout en désignant une autre URL canonique : directives potentiellement contradictoires.' },
  'canonical-target': { label: 'Canonical problématique', help: 'La canonical pointe vers une URL redirigée ou en erreur.' },
  'broken-link': { label: 'Lien interne cassé', help: 'Cette page contient un lien vers une URL qui répond en erreur (4xx/5xx). Un délai dépassé n\'est pas compté ici : il est signalé sur la page elle-même (Erreur réseau).' },
  'query-explosion': { label: 'Explosion de paramètres', help: 'Un même chemin existe en très nombreuses variantes de paramètres : risque de gaspillage de crawl.' },
  'sitemap-url-error': { label: 'Sitemap : URL inaccessible', help: 'Une URL listée dans le sitemap est en erreur.' },
  'sitemap-url-redirect': { label: 'Sitemap : URL redirigée', help: 'Une URL listée dans le sitemap redirige : le sitemap devrait contenir les URL finales.' },
  'sitemap-url-noindex': { label: 'Sitemap : URL en noindex', help: 'Une URL du sitemap demande à ne pas être indexée : signal contradictoire.' },
  'orphan-page': { label: 'Page orpheline', help: 'Listée au sitemap mais aucune page crawlée n’y mène par un lien.' },
  'not-in-sitemap': { label: 'Absente du sitemap', help: 'Page indexable trouvée par le crawl mais absente du sitemap.' },
  'noindex-header': { label: "Noindex (en-tête)", help: "Le serveur envoie X-Robots-Tag: noindex : la page demande à ne pas être indexée. Vérifiez que c'est voulu." },
  'title-too-long': { label: "Title trop long", help: "Title de plus de 60 caractères : il risque d'être tronqué dans les résultats. Seuil configurable (approximation en caractères)." },
  'title-too-short': { label: "Title trop court", help: "Title de moins de 30 caractères : il décrit peu la page. Seuil configurable." },
  'description-too-long': { label: "Description trop longue", help: "Meta description de plus de 160 caractères : elle risque d'être tronquée. Seuil configurable." },
  'description-too-short': { label: "Description trop courte", help: "Meta description de moins de 70 caractères. Seuil configurable." },
  'h2-missing': { label: "H2 absent", help: "Page de contenu sans sous-titre H2 : la structure du texte est peu lisible." },
  'thin-content': { label: "Contenu peu fourni", help: "Moins de 200 mots de texte visible. Seuil éditorial configurable." },
  'content-duplicate': { label: "Contenu dupliqué", help: "Plusieurs pages indexables ont exactement le même texte visible : les moteurs devront choisir laquelle indexer." },
  'hreflang-invalid-code': { label: "hreflang : code invalide", help: "Le code de langue/région d'un hreflang n'est pas valide (attendu : fr, fr-FR, x-default…) ; il sera ignoré." },
  'hreflang-no-self': { label: "hreflang : pas d'auto-référence", help: "La page déclare des hreflang mais ne se liste pas elle-même." },
  'hreflang-missing-return': { label: "hreflang : lien retour manquant", help: "La page cible un hreflang qui ne renvoie pas vers elle : Google ignore la paire." },
  'hreflang-target-error': { label: "hreflang : cible problématique", help: "Un hreflang pointe vers une URL redirigée, en erreur ou en noindex." },
  'insecure-http': { label: "Page en HTTP", help: "La page est servie sans chiffrement (HTTP)." },
  'hsts-missing': { label: "HSTS absent", help: "Pas d'en-tête Strict-Transport-Security : le navigateur n'est pas forcé à rester en HTTPS." },
  'nosniff-missing': { label: "X-Content-Type-Options absent", help: "Pas d'en-tête X-Content-Type-Options: nosniff (durcissement de sécurité)." },
  'mixed-content': { label: "Contenu mixte", help: "Une page HTTPS charge des images en HTTP : les navigateurs peuvent les bloquer. Seules les images sont examinées." },
  'slow-response': { label: "Réponse lente", help: "Le serveur met plus d'une seconde à répondre (premiers octets). Seuil configurable." },
  'lang-missing': { label: "Attribut lang absent", help: "La balise <html> n'a pas d'attribut lang (accessibilité et langue de la page)." },
  'viewport-missing': { label: "Meta viewport absente", help: "Sans meta viewport, l'affichage sur mobile est dégradé." },
  'image-alt-missing': { label: "Image sans alt", help: "Des images n'ont pas d'attribut alt (un alt vide reste valide pour une image décorative)." },
  'json-ld-invalid': { label: "JSON-LD invalide", help: "Un bloc de données structurées JSON-LD n'est pas du JSON valide (ou a été tronqué à 20 Ko) : il sera ignoré." },
  'json-ld-missing-property': { label: "JSON-LD : propriété requise absente", help: "Un type schema.org courant (Article, Product, Event…) n'a pas toutes les propriétés requises pour les résultats enrichis." },
  'content-near-duplicate': { label: "Contenu quasi dupliqué", help: "Plusieurs pages indexables ont un texte presque identique (simhash, au plus 3 bits d'écart). Vérifiez s'il faut les fusionner ou leur donner une canonical." },
  'sitemap-lastmod-invalid': { label: "Sitemap : lastmod invalide", help: "Le <lastmod> de cette URL n'est pas une date W3C (AAAA-MM-JJ ou date-heure avec fuseau) : les moteurs l'ignorent." },
  'anchor-empty': { label: "Ancre vide", help: "Un lien n'a aucun texte accessible (ni texte, ni alt d'image, ni aria-label) : les moteurs et les lecteurs d'écran ne savent pas où il mène." },
  'anchor-generic': { label: "Ancre générique", help: "Un lien a un texte comme « cliquez ici » ou « en savoir plus » : il ne décrit pas la page visée." },
  'external-link-broken': { label: "Lien externe cassé", help: "Un lien vers un autre site répond en erreur (4xx/5xx) ou est injoignable. Vérifié seulement avec l'option de vérification des liens externes ; les 401, 403 et 429 ne sont pas comptés." },
  'ai-snippet-blocked': { label: "IA : extrait bloqué", help: "Page indexable avec nosnippet ou max-snippet:0 : selon Google, une page doit pouvoir afficher un extrait pour servir de lien source dans AI Overviews et AI Mode. Vérifiez que c'est voulu." },
  'schema-faq-howto-retired': { label: "Schéma FAQ/HowTo sans effet", help: "Google a retiré les résultats enrichis FAQ et HowTo (2026) : le balisage reste valide mais n'a plus d'effet visible dans la recherche." },
  'ai-crawler-blocked': { label: "Robot IA interdit", help: "Bonne pratique GEO, non exigée par Google : le robots.txt interdit tout le site à un robot IA nommé (GPTBot, ClaudeBot, PerplexityBot, CCBot, Google-Extended, Applebot-Extended). Sans conséquence sur Google Search ; à vérifier si vous voulez être cité par ces assistants." },
  'article-author-missing': { label: "Article sans auteur", help: "Bonne pratique GEO/E-E-A-T, non exigée par Google : le JSON-LD Article n'indique pas d'auteur (author)." },
  'article-date-missing': { label: "Article sans date", help: "Bonne pratique GEO, non exigée par Google : le JSON-LD Article n'a ni datePublished ni dateModified (fraîcheur du contenu)." },
  'amp-target-error': { label: "AMP : version problématique", help: "Le lien amphtml pointe vers une page redirigée, en erreur ou en noindex." },
  'amp-no-canonical': { label: "AMP : canonical absente", help: "Une page AMP doit désigner sa version classique par une balise canonical (ou elle-même si elle est autonome)." },
  'microdata-no-type': { label: "Microdata sans itemtype", help: "Un élément itemscope n'a pas d'itemtype : les moteurs ne savent pas quel type de données il décrit." },
}

export function ruleInfo(rule: string): RuleInfo {
  return RULES[rule] ?? { label: rule, help: 'Règle sans description.' }
}

export const SEVERITY_LABEL: Record<Severity, string> = {
  Error: 'Erreur',
  Warning: 'Avertissement',
  Info: 'Info',
}

export const FIELD_LABEL: Record<string, string> = {
  status: 'Statut',
  redirect_to: 'Redirection',
  title: 'Title',
  description: 'Description',
  h1: 'H1',
  canonical: 'Canonical',
  robots: 'Meta robots',
  in_sitemap: 'Sitemap',
  error: 'Erreur',
}
