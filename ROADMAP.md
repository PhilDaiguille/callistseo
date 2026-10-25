# CallistSEO — Roadmap

Document de suivi, mis à jour à chaque jalon. Légende : ✅ fait · 🟡 partiel · ⬜ à faire · ⏸ reporté.
Dernière mise à jour : 20 oct. 2026.

## En un coup d'œil

Le moteur (crawl, règles, stockage, exports, API, interface web, desktop, Docker) fonctionne de bout en bout. L'itération « équivalent Screaming Frog payant » est faite aux deux tiers : il reste l'authentification des sites audités, les intégrations externes (PageSpeed, Search Console, GA4, IA) et les fonctions qui demandent un navigateur (rendu JavaScript).

| Bloc | Avancement | Détail |
| --- | --- | --- |
| Moteur de crawl (SSRF, robots, politesse, adaptatif) | ✅ | concurrence adaptative, pause / arrêt, périmètre par regex |
| Règles SEO | ✅ 59 règles | phase 3 faite, plus un lot IA/GEO 2026 (voir ci-dessous) ; reste : rendu JS, orthographe |
| Stockage et historique | ✅ | SQLite, migrations v1→v13, comparaison d'audits |
| API | ✅ | projets (création, suppression), audits, rapports, segments, planification, configuration |
| MCP (assistants IA) | ✅ | `callistseo mcp` : 13 outils (audits, rapports, comparaison, sitemap) ; reste : transport HTTP, ressources MCP |
| Interface web | ✅ | poste de travail, palette ⌘K, thème clair/sombre, mobile |
| Desktop (Tauri) | 🟡 | compile et fonctionne en processus ; release sur tag `v*` (Windows, macOS, Linux, brouillon GitHub) ; fenêtre, essais Windows/Linux, signature à valider |
| Docker / web | 🟡 | image durcie, connexion d'équipe (un mot de passe partagé, page `/login`) ; reste comptes nominatifs, HTTPS, image publiée |
| Site vitrine | ✅ | `site/index.html` autonome ; reste : URL du dépôt et domaine définitifs, publication |
| Performance et mémoire | ✅ | 49 Mo pour 6 000 pages denses, banc reproductible |
| Intégrations externes | ⬜ | phase 6 |
| Rendu JavaScript | ⏸ | phase 7 |

## Itération « Screaming Frog » : où on en est

| Phase | Contenu | État |
| --- | --- | --- |
| 1 | Vitesse et mémoire | ✅ concurrence adaptative (AIMD), URL partagées (346 → 49 Mo), stats en direct, test d'endurance, `cargo audit` ; détail dans `docs/perf-log.md` |
| 2 | Données par URL (schéma v3) | ✅ temps de réponse, taille, empreinte du texte, mots, langue, en-têtes utiles, hreflang, images, JSON-LD, H2, viewport |
| 3 | Règles SEO | ✅ 59 règles + sitemap XML |
| 4 | Interface « poste de travail » | ✅ 22 rapports, tableau virtualisé, inspecteur, ⌘K, pause / reprise / arrêt ; reste : résultats pendant l'audit, visualisations |
| 5 | Configuration, planification, segments | 🟡 faits : périmètre par projet, robots.txt d'essai, planification, segments, extraction personnalisée ; reste : authentification |
| 6 | Intégrations externes | ⬜ |
| 7 | Rendu JavaScript et le reste | ⏸ |

Déjà couvert de la liste Screaming Frog : liens cassés, titles et méta, directives robots, hreflang, doublons exacts, sitemaps XML (lecture et génération), données structurées (JSON-LD), recherche et extraction personnalisées, comparaison d'audits, robots.txt personnalisé, planification, segments, configuration du crawl, vitesse, accessibilité / mobile / AMP (sous-ensemble statique), sécurité (en-têtes, contenu mixte).

## Reste à faire

### Phase 3 — règles SEO restantes

- ~~Quasi-doublons de contenu (simhash par bandes)~~ fait (`content-near-duplicate`, migration v9).
- ~~Vérification des liens **externes**~~ fait (`external-link-broken`, option `--check-external` / `check_external` dans la config du projet ; 500 URL max, 8 hôtes en parallèle, une requête à la fois par hôte, ≥ 0,5 s d'écart, robots.txt de l'hôte respecté, redirections non suivies, 401/403/429/999 non comptés ; migration v12).
- ~~Détection AMP, microdata~~ fait (`amp-*`, `microdata-no-type`, migration v10 ; validation AMP complète hors périmètre), ~~propriétés requises du JSON-LD par type~~ fait (`json-ld-missing-property`, sous-ensemble de types).
- ~~`lastmod`~~ fait (`sitemap-lastmod-invalid`, migration v11) ; index de sitemaps déjà suivis (20 fichiers, 50 000 URL max). Ancrage : compte par page des liens sans texte accessible (`anchor-empty`) et à l'ancre générique (`anchor-generic`), pas le texte de chaque lien (non stocké : mémoire).
- ~~Benchmark à comptes exacts~~ fait pour les ancres, JSON-LD, microdata, AMP (canonical) et quasi-doublons ; `amp-target-error`, `sitemap-lastmod-invalid` et `external-link-broken` restent testées en unitaire/intégration (le banc n'a ni sitemap ni site tiers).

### Lot IA / GEO 2026

Basé sur la doc Search Central (aucune exigence spéciale pour AI Overviews / AI Mode : page indexée, snippet autorisé, crawl ouvert ; pas de `llms.txt` requis) : `ai-snippet-blocked`, `schema-faq-howto-retired` (officielles) ; `ai-crawler-blocked`, `article-author-missing`, `article-date-missing` (bonnes pratiques GEO, étiquetées comme non exigées par Google ; migration v13). Aucune règle « 2027 » : rien d'officiel n'est annoncé. Non faits : `llms.txt`, `data-nosnippet`, cohérence schéma / texte visible.

### Phase 4 — poste de travail

- Visualisations : arborescence des dossiers, graphe de liens plafonné (~3 000 nœuds).
- Accessibilité de l'interface à auditer (clavier, lecteur d'écran), tests de composants.

### Phase 5 — fin

- Authentification (voir ci-dessus).
- Comparaison automatique avec l'audit précédent poussée à l'utilisateur (notification / e-mail) ; calendrier précis (jour et heure) au lieu d'un simple intervalle.
- XPath pour l'extraction personnalisée.

### Phase 6 — intégrations (désactivées par défaut, vos clés)

- Écran de consentement (ce qui part chez qui), clés chiffrées, quotas et plafonds de coût.
- **PageSpeed Insights** : scores et Core Web Vitals mobile / desktop.
- **Search Console et GA4** par compte de service : clics, impressions, position, inspection d'URL, sessions.
- **OpenAI / Gemini** : analyse optionnelle sur échantillon, avec plafond de coût.
- Hors ressources du projet : métriques de liens (Ahrefs / Moz / Majestic), connecteur Looker Studio (remplacé par les exports).

### Phase 7 — plus tard

Rendu JavaScript (Chrome piloté + proxy appliquant le filtre anti-SSRF), JavaScript personnalisé, accessibilité et mobile complets (axe, Lighthouse), validation AMP complète, orthographe et grammaire.

### Distribution et projet

- **Desktop** : validation visuelle de la fenêtre et des exports natifs, builds Windows / Linux (produits par `release.yml` sur tag, jamais lancés sur une vraie machine), signature / notarisation, icône définitive, mise à jour automatique.
- **Docker / web** : comptes nominatifs et séparation des données (la connexion d'équipe par mot de passe partagé est faite), HTTPS (proxy), image publiée, builds multi-architectures, journal d'accès, limitation de débit, CSP sur l'interface.
- **Projet** : nom, marque, domaine et dépôt à valider juridiquement ; SECURITY.md et CONTRIBUTING.md ; documentation d'installation ; validation terrain (5 testeurs) ; bêta.
- **Qualité** : audit des règles sur davantage de vrais sites (faux positifs), `cargo deny`, corpus de fixtures (`tests/fixtures`), tests SSRF par redirection.

## Vision

Alternative open source (MIT) aux outils SEO payants : moteur de crawl natif en Rust, local-first, auto-hébergeable, desktop + web avec un maximum de code partagé. SEO technique d'abord, sémantique ensuite. Le cœur reste gratuit ; pas de version gratuite artificiellement bridée.

Hors périmètre initial : suite SEO complète, volumes de recherche propriétaires, backlinks exhaustifs, IA partout.

## Décisions retenues

| Sujet | Choix |
| --- | --- |
| Nom | CallistSEO (provisoire : vérifier marques, domaines, nom du dépôt avant publication) |
| Licence | MIT (vérifier les licences des dépendances) |
| Moteur / API | Rust stable, Axum |
| Frontend | React + TypeScript + Vite, shadcn/ui + Tailwind |
| Desktop | Tauri 2, avec backend Rust commun (service local en desktop, HTTP en web) |
| Stockage | SQLite (WAL, migrations versionnées) ; PostgreSQL à réévaluer si multi-utilisateur chargé |
| Déploiement | Docker Compose |
| Priorité | SEO technique avant la sémantique ; moteur de crawl maison |

## Socle technique

Workspace Cargo `core` / `crawler` / `seo-rules` / `storage` / `audit-diff` / `export` / `api` / `cli` + application desktop, 120 tests Rust + 34 tests front, CI GitHub Actions (fmt + clippy + test). Testé en réel sur https://bytemeuh.phildaiguille.fr/ : 68 URL, 0 erreur.

```
callistseo crawl <url> [--max-pages --max-depth --concurrency --allow-private --ignore-robots]
```

Sortie : `statut  profondeur  url  note  h1=N  title`. Code retour 0 / 1 (erreurs 4xx, 5xx ou réseau) / 2 (appel invalide).

Sécurité déjà en place : http/https seulement ; IP privées, loopback, link-local et réservées refusées après résolution DNS (anti-rebinding, connexion sur les adresses filtrées) ; pas de proxy système ; redirections non suivies en place (la cible repasse par la file) ; limites de pages, profondeur, concurrence, délai et taille de réponse.

## Backlog historique (issues 001–020)

### P0 — SEO technique

| # | Issue | État | Reste à faire |
| --- | --- | --- | --- |
| 001 | Monorepo Cargo + pnpm | 🟡 | `pnpm-workspace.yaml` quand le premier package JS arrive ; SECURITY.md, CONTRIBUTING.md |
| 002 | Types métier + normalisation d'URL | 🟡 | `CrawlRun`, `Issue` (avec le stockage et les règles) |
| 003 | Client HTTP avec limites de sécurité | ✅ | `--user-agent` / `CALLISTSEO_USER_AGENT` (validé), ciblage `robots.txt` sur le jeton de produit (RFC 9309). À faire : URL d'information officielle du robot une fois le dépôt public créé |
| 004 | File de crawl + déduplication | ✅ | File, dédup, `Crawl-delay`, `--delay-ms`, pause sur 429/503 (`Retry-After` en secondes, une reprise). Reste : politique 5xx de `robots.txt` (Google : tout interdit), `Retry-After` au format date HTTP |
| 005 | Découverte de liens + profondeur | ✅ | `<base href>`, `rel=nofollow` |
| 006 | Métadonnées et directives SEO | ✅ | `X-Robots-Tag`, hreflang, Open Graph si besoin |
| 007 | Corpus de fixtures + tests | 🟡 | Vrai dossier `tests/fixtures` (HTML invalides, gros documents, boucles) ; tests SSRF par redirection |
| 008 | Persistance SQLite | 🟡 | Crate `storage` faite (rusqlite embarqué) : projets, exécutions, pages, liens, problèmes ; migrations par `user_version`, WAL, index, sauvegarde atomique, exécutions jamais écrasées. Reste : politique de rétention, pagination/recherche SQL côté serveur (pour l'API), `CrawlRun`/`Issue` comme types métier dédiés |
| 009 | CLI de crawl | 🟡 | Sortie `--format text\|csv\|json` + `--out` faite (CSV neutralisé contre l'injection de formules). `--db` / `--project` (historique), `runs` et `export --run` faits. Reste : `diff` (issue 011), export HTML (issue 016) |
| 010 | Règles SEO + regroupement des problèmes | 🟡 | Crate `seo-rules` faite (HTTP, redirections/chaînes/boucles, title, description, H1, doublons, noindex, canonical, liens cassés, profondeur, explosion de paramètres). Sitemaps et pages orphelines faits. Validé sur 2 vrais sites ; un faux positif corrigé (liens Cloudflare `/cdn-cgi/` explorés à tort comme des liens cassés). Piste : exclure des règles de doublons les pages dont la canonical pointe vers une autre URL. Gravité harmonisée (Erreur = cassé ; Avertissement = à corriger ; Info = à vérifier) avec une source unique et `--fail-on`. Reste : regroupement par règle, seuils configurables en CLI, documentation des faux positifs |

### P1 — Application et historique

| # | Issue | État |
| --- | --- | --- |
| 011 | Snapshots + comparaison de deux audits (`audit-diff`) | ✅ crate pure + `callistseo diff --db … --before N --after M` (pages ajoutées/supprimées/modifiées, problèmes nouveaux/résolus ; code retour 1 si nouvelles erreurs, utilisable en CI). Reste : diff en JSON/CSV, tendances sur plusieurs audits |
| 012 | React, Vite, Tailwind, shadcn/ui | ✅ `apps/web` (React 19, Vite 8, Tailwind 4, shadcn/ui, TanStack Query, React Router), workspace pnpm, CI |
| 013 | Client API + pages projets | ✅ client typé (`src/lib/api.ts`, à extraire dans `packages/api-client` quand le desktop le partagera), projets, historique, lancement d'audit |
| 014 | Explorateur d'URL + centre de problèmes (pagination côté serveur, filtres dans l'URL) | 🟡 faits : tableau paginé avec recherche, filtre de statut, détail par URL, résumé des problèmes par règle avec explication, filtres dans l'URL, polling, annulation, exports. Comparaison de deux audits, maillage et détail d'URL (liens entrants/sortants) faits. Reste : graphe interactif (facultatif), tests de composants, accessibilité à auditer, mode sombre à valider |
| 015 | Tauri 2 + adaptateur desktop | 🟡 `apps/desktop` : l'application embarque le backend Rust et l'interface appelle l'API **en processus** (commande Tauri → `LocalApi` → routeur Axum), donc aucun port ouvert, aucun jeton, 100 % de l'API réutilisée. Export via boîte de dialogue native. Données dans le dossier d'application du système (`audits.db`). Testé sur macOS (build + lancement, interface embarquée, base créée). Reste : validation visuelle de la fenêtre et des exports natifs, builds Windows/Linux (non testés, job CI ajouté mais jamais exécuté), signature/notarisation, icône définitive, mise à jour automatique, identifiant `org.callistseo.desktop` à valider avec le nom du projet |
| 016 | Exports CSV et HTML | ✅ CSV (neutralisé contre l'injection de formules), JSON et rapport HTML autonome (échappement de tout contenu tiers, politique de sécurité embarquée interdisant les scripts, 200 lignes max par règle et 5 000 URL, explications de chaque règle) via la CLI, l'API et l'UI. Reste : export PDF éventuel, rapport personnalisable (logo, nom d'agence) |
| 017 | Docker Compose + configuration web (authentification, séparation des données) | 🟡 `deploy/` : image multi-étapes (interface + serveur Rust, ~ un seul conteneur), jeton obligatoire, volume de données, conteneur durci (non-root, lecture seule, sans capacités), sonde de santé, arrêt propre sur SIGTERM, interface servie par l'API (repli SPA, chemins sécurisés). Testé avec Docker 29 (OrbStack) sur architecture locale : audit réel, persistance après redémarrage, SSRF bloqué. Reste : comptes utilisateurs et séparation des données (un jeton partagé pour l'instant), HTTPS (proxy), image publiée (registre), builds multi-architectures, journal d'accès, limitation de débit, en-têtes de sécurité HTTP (CSP) sur l'interface |
| 018 | Benchmarks reproductibles (débit, mémoire max, latence ; machine et config documentées) | ✅ crate `callistseo-bench` : site synthétique déterministe (jusqu'à 50 000 pages), vrai binaire en processus séparé, mémoire mesurée par le système, latence p50/p95/p99, exactitude des règles contre une vérité terrain (54/54 comptes exacts), rapport dans `docs/benchmarks.md`. Constats sur Apple M5 : ≈ 42 000 pages/s en local, ≈ 2,1 Ko de mémoire par page (114 Mo pour 51 000 URL). Reste : mesures sur d'autres machines/OS, scénarios plus lourds (pages volumineuses, redirections), suivi des régressions dans la CI, comparaison avec d'autres outils si souhaitée |

### P2 — Sémantique et opportunités

| # | Issue | État |
| --- | --- | --- |
| 019 | Similarités et clusters de pages | ⬜ |
| 020 | Import Google Search Console (facultatif) | ⬜ |

## Dette connue

- `robots.txt` : un 5xx est traité comme « tout autorisé » ; `Retry-After` au format date HTTP ignoré (pause par défaut de 1 s).
- Un seul hôte crawlé par run (la politesse est donc globale à l'audit) ; pas de limite de débit commune entre audits simultanés de l'API.
- `<base href>` et `rel=nofollow` ignorés.
- NAT64 refusé en bloc (peut empêcher l'accès à certains sites IPv6-only).
- Pas de nom de dépôt / marque / domaine validé.
- Dépendances indirectes signalées par `cargo audit` (non maintenues ou fragiles : `fxhash`, `proc-macro-error`, `glib` 0.18) : elles viennent de Tauri et `scraper`, à lever avec leurs mises à jour.
- Les audits ne sont enregistrés qu'à leur fin : pendant un audit, le tableau est vide (pas de consultation en direct) et un audit très gros reste entièrement en mémoire.
- Un seul jeton partagé pour l'API (pas de comptes) ; les identifiants d'un site audité (Basic, cookies, formulaire) ne sont pas encore gérés.
- Banc à comptes exacts : le site synthétique n'a ni hreflang, ni sitemap, ni site tiers ; les règles correspondantes sont testées en unitaire. Les quasi-doublons ne sont pas vérifiés sur le corpus dense (remplissage identique).
- Les textes des règles existent en double (`seo-rules/src/info.rs` et `apps/web/src/lib/rules.ts`) : à garder synchronisé à la main.

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

### Plus tard

| # | Tâche | Pourquoi / contenu |
| --- | --- | --- |
| 1 | **Authentification des sites audités** (phase 5) | Basic auth, en-têtes et cookies, connexion par formulaire ; identifiants chiffrés (AES-GCM, clé `CALLISTSEO_SECRET_KEY`, fonction désactivée sans clé), jamais renvoyés ni journalisés, mêmes filtres SSRF |
| 2 | **Résultats pendant l'audit** | Enregistrer au fil de l'eau : tableau consultable en direct et mémoire bornée sur très gros sites (lève aussi une dette) |
