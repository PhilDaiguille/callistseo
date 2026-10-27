# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

CallistSEO : audit SEO open source (MIT), moteur de crawl en Rust, interface React, application desktop Tauri 2, déploiement Docker. Le code, les commentaires, l'interface et la documentation sont **en français** ; garder cette langue. `ROADMAP.md` est la source de vérité de l'état du projet (issues 001-020, dette connue) : le mettre à jour, ainsi que `README.md`, quand un changement modifie le comportement.

## Commandes

`make help` liste tout. L'essentiel :

```
make check                      # équivalent de la CI : fmt --check, clippy -D warnings, cargo test, build + tests web
cargo test -p callistseo-crawler robots                 # un test Rust (filtre par nom) dans une crate
cargo test -p callistseo-crawler --test crawl delay     # un test d'intégration
pnpm --filter web exec vitest run src/lib/format.test.ts  # un fichier de test web
make serve   &   make web-dev   # API 127.0.0.1:8080 + interface 5173 (proxy /api)
make serve-ui                   # API + interface construite sur un seul port (comme Docker)
make bench-quick                # échoue si une règle n'est pas exacte (comptes attendus connus)
```

- `cargo test` à la racine **n'inclut pas** l'app desktop (`default-members = crates/*`) : elle exige `apps/web/dist`. La compiler avec `cargo build -p callistseo-desktop` (ou `pnpm --filter desktop build`) après `make web-build`.
- `cargo test` ne recompile pas le binaire : avant de tester l'API/la CLI à la main, `cargo build -p callistseo-cli` (un binaire périmé a déjà donné de faux 404).
- Benchmarks : utiliser `--release` (`make bench`), les résultats debug sont sans valeur.

## Architecture

Flux : `crawler` (HTTP, file, parsing) → `seo-rules` (analyse pure) → `storage` (SQLite) → exposé par `api` (Axum) et `cli`, avec `export` (CSV/JSON/HTML) et `audit-diff` comme fonctions pures. `core` ne contient que les types (`CrawlConfig`, `CrawledPage`, `Issue`, `Severity`). Les erreurs sont des `String` dans tout le Rust.

Points qui demandent de lire plusieurs fichiers :

- **Sécurité SSRF** (`crawler/src/client.rs`) : un résolveur DNS filtre les IP privées et la connexion utilise les adresses filtrées (anti-rebinding) ; les IP littérales sont vérifiées à part (`check`) ; les redirections ne sont **jamais** suivies en place (la cible repasse par la file). `allow_private` n'existe que pour les tests et la CLI : l'API ne l'expose jamais.
- **Politesse** (`crawler/src/lib.rs`, `robots.rs`) : limiteur `next_slot` dans la boucle de lancement ; délai = max(`--delay-ms`, `Crawl-delay`), pause globale sur 429/503. `robots.txt` est ciblé sur le **jeton de produit** du user-agent (avant `/`), par égalité (RFC 9309).
- **Gravité des règles** : une seule source, `seo-rules/src/info.rs` (table `RULES`) ; `analyze` la lit. Un test fige la liste des règles « Erreur » (Erreur = cassé, Avertissement = à corriger, Info = à vérifier). La gravité est **stockée** avec chaque problème : changer la table ne modifie pas les anciens audits. `Issue::rule` est un `&'static str` (internalisé au rechargement depuis la base).
- **Règles sur les faits** : `seo-rules/src/extra.rs` (appelé en fin d'`analyze`). Un fait absent (`None`, audit d'avant la v3) ne déclenche jamais de règle. `is_indexable` (lib) définit les pages d'un sitemap.
- **Rapports et lignes** (`storage/src/reports.rs`) : `REPORTS` (prédicats SQL) et `COLUMNS` (expressions) sont une **liste blanche** ; l'API valide tout identifiant reçu contre elle (`/rows`) et ne met jamais une valeur dans le SQL. Ajouter un rapport = une entrée `Report` (un test vérifie que compteurs et lignes concordent). Les compteurs se calculent en un seul parcours (`report_counts`).
- **Périmètre d'audit** : `core::UrlScope` (dans `CrawlConfig.scope`) est compilé par `crawler/src/scope.rs` (`Scope::new` valide les regex : 50 motifs max, 500 caractères, taille compilée bornée). Il filtre les liens **découverts** (jamais l'URL de départ) ; les liens exclus restent dans le graphe mais ne sont pas explorés. L'API stocke la configuration du projet en JSON (`projects.config`, sans secret) ; priorité : demande de l'audit > projet > défauts du serveur, plafonnés par `ApiConfig`.
- **Planification** : table `schedules` (un par projet) ; `Scheduler::run` (api) passe toutes les 30 s, `run_due_schedules(state, now)` est testable avec une horloge injectée. Lancé par `serve` et, en desktop, par `LocalApi::scheduler()` (`tauri::async_runtime::spawn`). `LocalApi` accepte GET/POST/PUT/DELETE ; un 204 doit être reconstruit sans corps côté web (`transport.ts`).
- **Segments et extraction** : `SegmentDef` (storage/reports.rs) est un ensemble de critères dont chacun est une fraction de SQL **constante** + un paramètre (`sql(first)`), jamais de texte utilisateur dans la requête. Les extracteurs (`core::Extractor`, compilés par `crawler/src/extract.rs`) sont stockés par page dans `pages.extracted` (JSON `{nom: [valeurs]}`, plafonds dans `core`). Les refus métier de l'API sortent de la fermeture `db` (qui masque les vraies pannes en 500) pour garder leur code 409.
- **MCP** (`crates/mcp`, `callistseo mcp`) : serveur JSON-RPC sur stdio écrit à la main (pas de SDK). Aucune logique métier : chaque outil appelle `LocalApi` (mêmes protections que l'API). Stdout est réservé au protocole (journaux sur stderr). Pour ajouter un outil : une entrée dans `tools()` + une branche dans `run_tool` ; un test vérifie que chaque outil a un schéma valide et qu'aucun outil destructeur n'est exposé.
- **Liens internes précalculés** : `pages.inlinks` / `outlinks` (migration v8) sont écrits à la fin de l'enregistrement d'un audit (`insert_all`) ; le maillage, les tris et les rapports les lisent. Ne pas les recalculer à la lecture (jointure sur `links` : 1 à 3 s sur 3 000 pages denses). Un nouvel `ORDER BY` sur `links` doit être vérifié avec `EXPLAIN QUERY PLAN` (un tri forcé annule le `LIMIT`).
- **HTTP** : `http_app` = `router` + compression gzip/brotli + `Cache-Control` (`/assets/*` immuable sauf repli HTML, `/api/*` no-store) ; `serve` l'utilise, pas `LocalApi`.
- **Poste de travail** (`apps/web/src/pages/Workstation.tsx`, `components/workstation/`) : tout l'état d'affichage est dans l'URL (`r`, `sort`, `dir`, `cols`, `q`, `sel`) ; tableau virtualisé (`@tanstack/react-virtual`) chargé par tranches de 200 ; `lib/table.ts` porte la logique pure testée.
- **Pages en direct** : `crawl_live` envoie chaque page sur un canal ; `write_live_pages` (api) les enregistre par lots (`add_live_pages`, sans liens ni problèmes, ignoré si l'audit n'est plus `running`). `finish_run` efface ces lignes et écrit le résultat complet ; annulation / échec les effacent. Pendant l'audit, `inlinks` vaut 0 et il n'y a aucun problème : l'interface relit le tableau toutes les 3 s et ouvre « Toutes les URL ».
- **Pause / arrêt** : `CrawlStats.paused` / `.stop` (atomiques) lus par la boucle du crawler ; l'arrêt rend les pages déjà explorées (enregistrées normalement), « annuler » abandonne la tâche.
- **Textes des règles en double** : `seo-rules/src/info.rs` et `apps/web/src/lib/rules.ts` — à garder synchronisés à la main (seul le côté Rust est testé).
- **Faits par URL** (`core::PageFacts`, schéma v3) : le crawler les remplit en une passe (`client.fetch` pour la réponse, `parse_html` pour le contenu). Ajouter un fait = champ dans `PageFacts` + colonne dans une **nouvelle** migration + `PAGE_COLS`/`read_facts`/`INSERT` du stockage (les listes sont du JSON, NULL si vides ; les audits anciens relisent des valeurs vides). L'empreinte (`content_hash`) porte sur le texte visible, pas sur le HTML.
- **Stockage** (`storage/src/lib.rs`) : migrations append-only via `PRAGMA user_version` (`MIGRATIONS`), exécutions jamais écrasées, états `running/done/failed/cancelled`.
- **API** (`api/src/lib.rs`) : `AppState.db()` exécute SQLite dans `spawn_blocking` ; l'authentification (jeton Bearer) couvre `/api/*` sauf `/api/health` ; `static_dir` sert l'interface avec repli SPA ; les audits tournent en tâches de fond (`spawn_crawl`, registre `running` pour annuler/suivre). `LocalApi` appelle le même routeur **en processus** : c'est ce qu'utilise l'app desktop (aucun port ouvert).
- **Frontend** (`apps/web`) : `src/lib/transport.ts` choisit `fetch` (navigateur) ou la commande Tauri `api_request` (desktop) en renvoyant une `Response` standard ; filtres et pagination vivent dans l'URL (`useQueryParams`) ; TanStack Query pour le polling. shadcn/ui style `base-nova` (Base UI, pas Radix) ; alias `@` déclaré dans `tsconfig.json` **et** `tsconfig.app.json` (sinon le CLI shadcn écrit dans un dossier `@/`).
- **Connexion d'équipe** : le jeton du serveur est le mot de passe partagé ; `LoginPage` le vérifie via `/api/projects`, `fail()` (`lib/api.ts`) renvoie vers `/login?next=` sur un 401 (jamais en desktop), `safeNext` n'accepte qu'un chemin interne. Pas de table d'utilisateurs.
- **Vitrine** (`site/index.html`) : page publique autonome (aucun script, polices dans `site/fonts`), hors de l'app ; couleurs copiées de `index.css`.
- **Design** (`docs/design.md`) : jetons et classes communes (`.panel`, `.table-panel`, `.panel-pop`, `.field`) dans `apps/web/src/index.css` ; hiérarchie fort/léger/neutre à respecter (pas de contour épais partout). Les composants de `components/ui/` sont adaptés dans le dépôt : un `shadcn add` les écraserait. Changer la palette : `src/lib/contrast.test.ts` impose le contraste AA. Polices embarquées (pas de CDN). Thème clair/sombre/auto dans `src/lib/theme.ts`.
- **Bench** (`crates/bench`) : site synthétique déterministe avec défauts injectés en nombre connu (`site::expected`), lance le vrai binaire `callistseo` en processus séparé pour mesurer la mémoire.

## Tests

Les tests d'intégration du crawler (`crawler/tests/crawl.rs`) démarrent un mini-serveur TCP en processus : tableau de `Route = (chemin, statut, en-têtes, corps)`, `{PORT}` est remplacé dans les corps, `serve_with` pour un serveur à état. Les tests API utilisent `tower::ServiceExt::oneshot` sur le routeur (sans réseau). Les tests de timing n'assertent que des bornes basses.

## À savoir

- `deploy/.env` contient un jeton : jamais dans git (`.gitignore` ignore `.env`, pas `.env.example`).
- L'identifiant du robot n'a volontairement aucune URL par défaut (`CallistSEO/<version>`) ; l'opérateur ajoute la sienne via `--user-agent` / `CALLISTSEO_USER_AGENT`.
- Le nom « CallistSEO », l'identifiant Tauri `org.callistseo.desktop` et l'URL de contact du robot ne sont pas validés juridiquement (voir `ROADMAP.md`).
