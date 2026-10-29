# CallistSEO — Roadmap

Document de suivi, mis à jour à chaque jalon. Légende : ✅ fait · 🟡 partiel · ⬜ à faire · ⏸ reporté.
Dernière mise à jour : 29 oct. 2026.

## En un coup d'œil

Le moteur (crawl, règles, stockage, exports, API, interface web, desktop, Docker) fonctionne de bout en bout. L'itération « équivalent Screaming Frog payant » est faite aux deux tiers : il reste l'authentification des sites audités, les intégrations externes (PageSpeed, Search Console, GA4, IA) et les fonctions qui demandent un navigateur (rendu JavaScript). Le projet est en construction, avant bêta.

| Bloc | État | Détail |
| --- | --- | --- |
| Moteur de crawl (SSRF, robots, politesse, adaptatif) | ✅ | concurrence adaptative, pause / arrêt, périmètre par regex |
| Règles SEO | ✅ 59 règles | dont un lot IA/GEO 2026 ; reste : rendu JS, orthographe |
| Stockage et historique | ✅ | SQLite, migrations v1→v14, comparaison d'audits |
| API | ✅ | projets, audits, rapports, segments, planification, configuration |
| MCP (assistants IA) | ✅ | `callistseo mcp` : 13 outils ; reste : transport HTTP, ressources |
| Interface web | ✅ | poste de travail, palette ⌘K, thème clair/sombre, mobile, tableau de bord (score de santé, tendance, gravités), barre latérale |
| Desktop (Tauri) | 🟡 | fonctionne en processus ; release sur tag `v*` ; reste : essais Windows/Linux, signature |
| Docker / web | 🟡 | image durcie et publiée (ghcr), HTTPS par Caddy, CSP, limite de connexion, journal d'accès ; reste : comptes |
| Site vitrine | ✅ | `site/index.html` autonome ; reste : domaine et publication |
| Performance et mémoire | ✅ | 49 Mo pour 6 000 pages denses, banc reproductible |
| Intégrations externes | 🟡 | phase 6 (code et tests faits, reste la validation contre les vraies API) |
| Rendu JavaScript | ⏸ | phase 7 |

## Prochaines étapes

### Priorité 1 — avant d'ouvrir le projet (débloque la bêta)

1. **Nom, dépôt, domaine** à valider (marques, juridique), puis URL d'information du robot (`--user-agent`), identifiant Tauri `org.callistseo.desktop`, domaine du site vitrine.
2. ~~Documents de projet~~ faits : `SECURITY.md`, `CONTRIBUTING.md`, `docs/installation.md` (adresse de contact de sécurité : signalement privé GitHub, à confirmer une fois le dépôt définitif).
3. ~~Déploiement web~~ fait : image `ghcr.io` amd64/arm64 publiée sur tag `v*` (`docker.yml`, jamais lancé), profil `https` (Caddy) dans le compose, CSP et en-têtes de sécurité, 10 échecs de connexion par minute et par adresse, journal d'accès. Reste : premier tag pour vérifier la publication.
4. **Desktop** : essais réels Windows / Linux (builds de `release.yml` jamais lancés sur une vraie machine), validation visuelle de la fenêtre et des exports natifs, signature / notarisation, icône définitive (mise à jour : étape 5 bis).
5. **Validation terrain** : 5 testeurs professionnels, dont 3 réutilisent l'outil sur un autre site.
5 bis. 🟡 **Mise à jour du service** : web fait (version dans l'interface et `/api/health`, `docker compose pull && up -d` documenté, migrations automatiques avec copie `audits.db.avant-v<N>.bak`, retour arrière documenté). Desktop : vérification au démarrage (updater Tauri, bannière, installation sur clic) ; un appel sortant vers GitHub Releases à chaque lancement. Reste : générer la clé de signature (`pubkey` dans `tauri.conf.json`, secrets `TAURI_SIGNING_PRIVATE_KEY` et `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`) et publier la release (le brouillon n'est pas visible de `releases/latest`).

### Priorité 2 — valeur produit

6. 🟡 **Résultats pendant l'audit** : ~~tableau en direct~~ fait (pages enregistrées par lots pendant l'audit, remplacées par le résultat complet à la fin ; problèmes, liens entrants et maillage restent calculés à la fin). Reste : mémoire bornée sur très gros sites (l'analyse a besoin de toutes les pages : la faire lire depuis la base).
7. ~~Authentification des sites audités~~ faite : Basic, en-têtes, cookies et **connexion par formulaire** (page chargée, jeton CSRF et champs cachés repris, cookies de session gardés, liens de déconnexion jamais suivis), tout envoyé à la seule origine du projet ; chiffrés (AES-256-GCM, `CALLISTSEO_SECRET_KEY`, fonction désactivée sans clé ; desktop : clé locale `secret.key`), jamais renvoyés ni journalisés. Reste en option : clé desktop dans le trousseau du système, connexions en JavaScript (phase 7).
8. 🟡 **Qualité des règles** : faits : tests SSRF par redirection, corpus `crates/crawler/tests/fixtures` (HTML invalide, métadonnées absurdes, document vide, JSON servi en HTML, page de 7 Mo, 50 000 niveaux d'imbrication, redirection vers soi, `Location` invalide, faux gzip), `cargo deny` dans la CI (licences, failles, sources ; a corrigé RUSTSEC-2026-0384 dans tokio), banc à comptes exacts dans la CI, seuils réglables en CLI (`--title-max`, `--min-words`…). Le corpus a révélé un temps quadratique de l'analyseur HTML sur l'imbrication profonde (8 s pour 100 000 niveaux) : document coupé au-delà de 1 000 niveaux. Reste : audit sur 10+ vrais sites (faux positifs) et leur documentation, détection de régressions de performance (le banc ne vérifie que l'exactitude).
9. ~~Robustesse robots~~ fait : `robots.txt` en 5xx = tout interdit (comme Google ; `--ignore-robots` le contourne), `Retry-After` en date HTTP, `<base href>` pris en compte. `rel=nofollow` reste volontairement exploré (le lien reste dans le graphe) ; compté dans `nofollow_links`.
10. **Suivi des régressions** : comparaison avec l'audit précédent poussée à l'utilisateur (webhook / e-mail), calendrier jour et heure au lieu d'un simple intervalle, tendances sur plusieurs audits, diff en JSON/CSV.

### Priorité 3 — différenciation

11. **Intégrations** (désactivées par défaut, vos clés) : écran de consentement (ce qui part chez qui), clés chiffrées, quotas et plafonds de coût. **PageSpeed Insights** (Core Web Vitals), puis **Search Console et GA4** (compte de service), puis **OpenAI / Gemini** sur échantillon avec plafond.
12. 🟡 **Visualisations** : ~~score de santé, courbe de tendance, répartition par gravité~~ faits (Recharts chargé à la demande). Reste : score par catégorie, arborescence des dossiers, graphe de liens plafonné (~3 000 nœuds).
13. **Règles à faible coût** : `data-nosnippet`, `llms.txt` (informatif), cohérence schéma / texte visible. ~~XPath pour l'extraction personnalisée~~ fait (`xpath`, via `skyscraper`).
14. **MCP** : transport HTTP et ressources MCP.
15. **Interface** : audit d'accessibilité (clavier, lecteur d'écran), tests de composants, synchronisation automatique `info.rs` ↔ `rules.ts` (test ou génération).
16. **Rapports** : HTML personnalisable (logo, agence), export PDF.
17. **Sémantique** (issues 019-020) : similarités et clusters de pages, import Search Console.

### Plus tard (phase 7)

Rendu JavaScript (Chrome piloté + proxy appliquant le filtre anti-SSRF), JavaScript personnalisé, accessibilité et mobile complets (axe, Lighthouse), validation AMP complète, orthographe et grammaire. Hors ressources du projet : métriques de liens (Ahrefs / Moz / Majestic), connecteur Looker Studio (remplacé par les exports).

## Fait

| Phase | Contenu | État |
| --- | --- | --- |
| 1 | Vitesse et mémoire : concurrence adaptative (AIMD), URL partagées (346 → 49 Mo), stats en direct, test d'endurance, `cargo audit` (détail : `docs/perf-log.md`) | ✅ |
| 2 | Données par URL (schéma v3) : temps de réponse, taille, empreinte du texte, mots, langue, en-têtes, hreflang, images, JSON-LD, H2, viewport | ✅ |
| 3 | 59 règles SEO + sitemap XML : quasi-doublons (simhash), liens externes (`--check-external`), AMP, microdata, JSON-LD par type, `lastmod`, ancres, benchmark à comptes exacts | ✅ |
| 4 | Poste de travail : 22 rapports, tableau virtualisé, inspecteur, ⌘K, pause / reprise / arrêt | ✅ |
| 5 | Configuration : périmètre par projet, robots.txt d'essai, planification, segments, extraction personnalisée (regex, CSS, XPath, recherche), sites protégés (Basic, en-têtes, cookies, formulaire) | ✅ |
| 6 | Intégrations externes : PageSpeed, Search Console, GA4 (compte de service), clés chiffrées, écran de consentement, règles CWV / impressions. Reste : essai sur de vrais comptes Google, OpenAI / Gemini | 🟡 |
| 7 | Rendu JavaScript et le reste | ⏸ |

**Lot IA / GEO 2026** : `ai-snippet-blocked`, `schema-faq-howto-retired` (officielles, doc Search Central) ; `ai-crawler-blocked`, `article-author-missing`, `article-date-missing` (bonnes pratiques GEO, étiquetées comme non exigées par Google). Aucune exigence spéciale de Google pour AI Overviews / AI Mode, pas de `llms.txt` requis, aucune règle « 2027 » (rien d'officiel annoncé).

Couvert de la liste Screaming Frog : liens cassés, titles et méta, directives robots, hreflang, doublons exacts, sitemaps XML (lecture et génération), données structurées, recherche et extraction personnalisées, comparaison d'audits, robots.txt personnalisé, planification, segments, configuration du crawl, vitesse, accessibilité / mobile / AMP (sous-ensemble statique), sécurité (en-têtes, contenu mixte).

## Socle technique

Workspace Cargo `core` / `crawler` / `seo-rules` / `storage` / `audit-diff` / `export` / `api` / `cli` / `mcp` / `bench` + application desktop ; environ 140 tests Rust et 47 tests front ; CI GitHub Actions (fmt + clippy + test). Testé en réel sur https://bytemeuh.phildaiguille.fr/ : 68 URL, 0 erreur.

```
callistseo crawl <url> [--max-pages --max-depth --concurrency --allow-private --ignore-robots]
```

Sortie : `statut  profondeur  url  note  h1=N  title`. Code retour 0 / 1 (erreurs 4xx, 5xx ou réseau) / 2 (appel invalide).

Sécurité en place : http/https seulement ; IP privées, loopback, link-local et réservées refusées après résolution DNS (anti-rebinding, connexion sur les adresses filtrées) ; pas de proxy système ; redirections non suivies en place (la cible repasse par la file) ; limites de pages, profondeur, concurrence, délai, taille de réponse et imbrication HTML (1 000 niveaux).

## Backlog historique (issues 001–020)

| # | Issue | État | Notes |
| --- | --- | --- | --- |
| 001 | Monorepo Cargo + pnpm | ✅ | `SECURITY.md`, `CONTRIBUTING.md` ajoutés |
| 002 | Types métier + normalisation d'URL | 🟡 | reste : `CrawlRun`, `Issue` comme types dédiés |
| 003 | Client HTTP avec limites de sécurité | ✅ | `--user-agent` validé, `robots.txt` ciblé sur le jeton de produit (RFC 9309) ; URL du robot : étape 1 |
| 004 | File de crawl + déduplication | ✅ | `Crawl-delay`, `--delay-ms`, pause sur 429/503 (`Retry-After` secondes ou date) |
| 005 | Découverte de liens + profondeur | ✅ | `<base href>` pris en compte ; `rel=nofollow` exploré volontairement |
| 006 | Métadonnées et directives SEO | ✅ | `X-Robots-Tag`, hreflang ; Open Graph si besoin |
| 007 | Corpus de fixtures + tests | ✅ | tests SSRF, corpus `crates/crawler/tests/fixtures` (pages piégées) |
| 008 | Persistance SQLite | 🟡 | reste : politique de rétention, `CrawlRun` / `Issue` dédiés |
| 009 | CLI de crawl | ✅ | `--format text\|csv\|json`, `--db` / `--project`, `runs`, `export`, `diff` |
| 010 | Règles SEO + regroupement des problèmes | 🟡 | gravité harmonisée (source unique, `--fail-on`) ; seuils réglables en CLI ; reste : doc des faux positifs (étape 8) ; piste : exclure des doublons les pages dont la canonical pointe ailleurs |
| 011 | Snapshots + comparaison (`audit-diff`) | ✅ | `callistseo diff` (code retour 1 si nouvelles erreurs, utilisable en CI) ; reste : étape 10 |
| 012 | React, Vite, Tailwind, shadcn/ui | ✅ | `apps/web` |
| 013 | Client API + pages projets | ✅ | client typé dans `src/lib/api.ts` (à extraire en package si besoin) |
| 014 | Explorateur d'URL + centre de problèmes | 🟡 | reste : graphe, tests de composants, accessibilité (étapes 12, 15) |
| 015 | Tauri 2 + adaptateur desktop | 🟡 | backend en processus (`LocalApi`), aucun port ouvert ; testé sur macOS ; reste : étape 4 |
| 016 | Exports CSV, JSON, HTML | ✅ | CSV neutralisé contre l'injection de formules, HTML autonome sans script ; reste : étape 16 |
| 017 | Docker Compose + web | 🟡 | conteneur durci, jeton obligatoire, sonde de santé, HTTPS (Caddy), image publiée ; testé Docker 29 ; reste : comptes nominatifs |
| 018 | Benchmarks reproductibles | ✅ | `callistseo-bench`, 54/54 comptes exacts, ≈ 42 000 pages/s, ≈ 2,1 Ko/page (Apple M5) ; reste : autres machines, scénarios lourds, CI (étape 8) |
| 019 | Similarités et clusters de pages | ⬜ | étape 17 |
| 020 | Import Google Search Console | ⬜ | étape 11 |

## Dette connue

- Un seul hôte crawlé par run ; pas de limite de débit commune entre audits simultanés de l'API.
- NAT64 refusé en bloc (peut empêcher l'accès à certains sites IPv6-only).
- Un audit très gros reste entièrement en mémoire jusqu'à la fin (analyse en mémoire ; étape 6).
- Un seul jeton partagé pour l'API (pas de comptes).
- Banc à comptes exacts : le site synthétique n'a ni hreflang, ni sitemap, ni site tiers (règles testées en unitaire) ; quasi-doublons non vérifiés sur le corpus dense.
- Textes des règles en double (`seo-rules/src/info.rs` et `apps/web/src/lib/rules.ts`), synchronisés à la main (étape 15).
- Dépendances indirectes signalées comme non maintenues (`fxhash`, `proc-macro-error`, `glib` 0.18) : elles viennent de Tauri et `scraper` ; `cargo deny` (CI) ne fait échouer que les failles.

## Vision et décisions

Alternative open source (MIT) aux outils SEO payants : moteur de crawl natif en Rust, local-first, auto-hébergeable, desktop + web avec un maximum de code partagé. SEO technique d'abord, sémantique ensuite. Le cœur reste gratuit ; pas de version gratuite artificiellement bridée. Hors périmètre initial : suite SEO complète, volumes de recherche propriétaires, backlinks exhaustifs, IA partout.

| Sujet | Choix |
| --- | --- |
| Nom | CallistSEO (provisoire : vérifier marques, domaines, nom du dépôt avant publication) |
| Licence | MIT (vérifier les licences des dépendances) |
| Moteur / API | Rust stable, Axum |
| Frontend | React + TypeScript + Vite, shadcn/ui + Tailwind |
| Desktop | Tauri 2, backend Rust commun (service local en desktop, HTTP en web) |
| Stockage | SQLite (WAL, migrations versionnées) ; PostgreSQL à réévaluer si multi-utilisateur chargé |
| Déploiement | Docker Compose |
| Priorité | SEO technique avant la sémantique ; moteur de crawl maison |

## Risques

| Risque | Réponse |
| --- | --- |
| Scope trop large | Crawl et SEO technique d'abord |
| Faux positifs SEO | Règles documentées, fixtures, niveau de confiance |
| Explosion d'URL | Limites strictes, déduplication, arrêt contrôlé |
| SSRF (surtout en web) | Validation d'URL, DNS, redirections ; tests dédiés |
| Mémoire | Lecture en flux, limites de taille, benchmarks |
| Desktop et web divergent | Backend commun, contrats explicites |
| SQLite insuffisant | Mesurer avant de migrer |
| Sémantique coûteuse | Commencer par la similarité de contenu, mesurer |
| Maintenance open source | Doc, contribution guidée, tests automatisés |

## Critères de réussite

| Indicateur | Objectif |
| --- | --- |
| Exactitude des règles | ≥ 99 % sur le corpus de tests (règles déterministes) |
| Sécurité | Tests SSRF et limites de ressources au vert |
| Reproductibilité | Même config + même fixture = résultats équivalents |
| Robustesse | Erreurs réseau, redirections, HTML invalide : jamais de plantage |
| Performance | Benchmarks reproductibles, mémoire / débit / latence mesurés |
| Installation | Procédures desktop et web documentées et testées |
| Validation terrain | 5 testeurs pro, dont 3 réutilisent l'outil sur un autre site |
