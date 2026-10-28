# CallistSEO

Alternative open source (MIT) aux outils d'audit SEO payants : moteur de crawl en Rust, local-first, auto-hébergeable.
Nom de travail : disponibilité juridique/domaine à vérifier avant publication.
Code source : <https://github.com/PhilDaiguille/callistseo> · Licence : [MIT](LICENSE)

```
git clone git@github.com:PhilDaiguille/callistseo.git && cd callistseo
```

## État

Projet en construction, avant bêta : moteur de crawl, règles SEO, CLI, API, interface web, application desktop et déploiement Docker fonctionnent (voir [`ROADMAP.md`](ROADMAP.md) pour l'état précis et ce qui reste à faire).

```
cargo run -p callistseo-cli -- crawl https://example.com --max-pages 500 --max-depth 5 --concurrency 8
```

Politesse : `--delay-ms N` impose un délai minimal entre deux requêtes ; le `Crawl-delay` de robots.txt est appliqué automatiquement (le plus grand des deux l'emporte, plafonné à 30 s ; `--ignore-robots` le désactive). Si le serveur répond 429 ou 503, tout le crawl se met en pause (`Retry-After`, ou 1 s) et l'URL est retentée une fois.

Sortie texte : une ligne par URL (`statut  profondeur  url  note  h1  title`), puis les problèmes détectés (gravité, règle, URL, message).
Exports : `--format csv --out ./rapport` (écrit `pages.csv` et `issues.csv`) ou `--format html` (rapport autonome : un seul fichier, sans script ni ressource externe ; stdout, ou `report.html` avec `--out`) ou `--format json` (stdout, ou `report.json` avec `--out`).
Historique (SQLite) : `--db audits.db [--project nom]` enregistre chaque audit sans jamais écraser les précédents ; `callistseo runs --db audits.db` les liste ; `callistseo export --db audits.db --run 2 --format csv --out ./rapport` les réexporte.
API HTTP : `callistseo serve --db audits.db --token "$(openssl rand -hex 16)"` (écoute sur `127.0.0.1:8080` ; hors loopback, `--token` est obligatoire ; jeton aussi lisible via `CALLISTSEO_TOKEN`). Toutes les requêtes portent `Authorization: Bearer <jeton>`.

| Méthode | Route | Rôle |
| --- | --- | --- |
| `POST` / `GET` | `/api/projects` | créer (`{"name","start_url"}`) / lister |
| `GET` | `/api/projects/{id}` | projet et ses audits |
| `POST` | `/api/projects/{id}/crawls` | lancer un audit (`{"max_pages","max_depth","concurrency"}` facultatifs, plafonnés côté serveur) → 202 |
| `GET` | `/api/crawls/{id}` | état (`running`, `done`, `failed`, `cancelled`) et avancement |
| `GET` | `/api/crawls/{id}/pages?search=&status=&limit=&offset=` | pages paginées |
| `GET` | `/api/crawls/{id}/issues?severity=&rule=&limit=&offset=` | problèmes paginés + résumé par règle |
| `GET` | `/api/crawls/{id}/graph` | nœuds et arêtes du maillage interne |
| `GET` | `/api/crawls/{avant}/diff/{après}` | comparer deux audits terminés d'un même projet |
| `GET` | `/api/crawls/{id}/page?url=` | détail d'une URL : métadonnées, problèmes, liens entrants et sortants |
| `GET` | `/api/crawls/{id}/links` | maillage : profondeur, pages les plus liées, pages orphelines |
| `POST` | `/api/crawls/{id}/cancel` | annuler un audit en cours |
| `GET` | `/api/crawls/{id}/export?format=json\|csv\|html&kind=pages\|issues` | export |

L'API ne permet jamais de viser le réseau local : le filtre SSRF du crawler reste actif.

Comparaison : `callistseo diff --db audits.db --before 1 --after 2` (code retour 1 si de nouvelles erreurs sont apparues).
Seuils des règles éditoriales réglables : `--title-min`, `--title-max`, `--description-min`, `--description-max`, `--min-words`, `--slow-ms`, `--deep-page`, `--max-query-variants` (voir `callistseo crawl --help`).

Code retour de `crawl` : 0 si le seuil `--fail-on` n'est pas atteint, 1 sinon, 2 si l'appel est invalide. `--fail-on error` (défaut : au moins une erreur), `warning` (erreur ou avertissement) ou `never`.

## Gravité des règles

Trois niveaux, avec un critère explicite (`callistseo rules` liste toutes les règles) :

| Gravité | Sens | Règles |
| --- | --- | --- |
| **Erreur** | quelque chose est *cassé*, de façon objective ; fait échouer une CI par défaut | erreur réseau, erreur HTTP (4xx/5xx), boucle de redirections, lien interne cassé, URL du sitemap inaccessible |
| **Avertissement** | problème d'optimisation ou d'indexation probable, à corriger ; rien n'est cassé | title absent/vide/dupliqué, description absente/vide, H1 absent, canonical vers une URL redirigée ou en erreur, noindex + canonical, URL du sitemap redirigée ou en noindex, chaîne de redirections, page orpheline, explosion de paramètres |
| **Info** | constat neutre ou éditorial, à vérifier | redirection, noindex, H1 multiples, page profonde, page absente du sitemap, description dupliquée |

La gravité est définie à un seul endroit (`crates/seo-rules/src/info.rs`) et un test empêche d'ajouter une « Erreur » sans le décider explicitement. Ce sont des constats techniques, pas des pénalités certaines des moteurs de recherche. Les audits déjà enregistrés gardent la gravité qu'ils avaient au moment de leur création.

## Interface web

```
cargo run -p callistseo-cli -- serve --db audits.db   # API sur 127.0.0.1:8080
pnpm install && pnpm web:dev                          # interface sur http://127.0.0.1:5173 (proxy /api -> 8080)
```

Projets, lancement et suivi d'audits, explorateur d'URL (recherche, filtre de statut, détail), centre de problèmes par règle, exports.

### Accès d'équipe

Une entreprise installe CallistSEO une fois et toute l'équipe partage **un même accès** : le jeton du serveur (`--token` / `CALLISTSEO_TOKEN`) sert de mot de passe d'équipe. Choisissez-en un long (16 caractères minimum, ex. `openssl rand -hex 16`) et transmettez-le à l'équipe.

- Toute page de l'interface renvoie vers `/login` si l'accès manque ou est refusé, puis revient à la page demandée.
- « Se souvenir de moi » garde l'accès sur ce poste (`localStorage`) ; sinon il est oublié à la fermeture de l'onglet. « Se déconnecter » l'efface.
- Changer le mot de passe = redémarrer le serveur avec un nouveau jeton : tout le monde est déconnecté.
- Sans `--token` (usage local) et dans l'application desktop, il n'y a pas d'écran de connexion.

Pas de comptes nominatifs (on ne sait pas qui a fait quoi, on ne retire pas l'accès d'une seule personne) : à ajouter si le besoin apparaît.

### Sites protégés

Préproduction derrière un mot de passe, espace réservé : un projet peut porter des identifiants (onglet Configuration → « Site protégé »), **authentification HTTP Basic**, **en-têtes** (clé d'accès, `Authorization: Bearer …`), **cookies** et **connexion par formulaire** : avant l'audit, la page de connexion est chargée, son formulaire rempli (jeton CSRF et champs cachés repris, champs identifiant et mot de passe détectés ou nommés) et envoyé ; les cookies de session servent pour tout l'audit, et les liens de déconnexion (`logout`, `deconnexion`…) ne sont jamais suivis. Identifiants refusés (formulaire réaffiché) : l'audit échoue avec un message clair.

- Envoyés **uniquement** à l'origine du projet (même protocole, hôte et port) : jamais aux liens externes vérifiés, jamais à la version HTTP d'un site HTTPS. Les filtres anti-SSRF restent les mêmes.
- Chiffrés en base (AES-256-GCM, liés au projet) avec `CALLISTSEO_SECRET_KEY` (`openssl rand -hex 32`) ; sans clé, la fonction est désactivée. L'API ne renvoie que leur nature (Basic, nombre d'en-têtes, cookies), jamais leurs valeurs ; rien n'est journalisé ni enregistré avec l'audit.
- Clé changée ou perdue : les identifiants deviennent illisibles, les audits du projet sont refusés (plutôt que de crawler la page de connexion) jusqu'à ce qu'on les ressaisisse ou les retire.
- Desktop : clé créée au premier lancement (`secret.key`, dans le dossier de l'application). CLI : `--basic-auth utilisateur:motdepasse` (ou `CALLISTSEO_BASIC_AUTH`, absent de la liste des processus), `--header "Nom: valeur"`, `--cookie "nom=valeur"`, `--login-url … --login-user …` (mot de passe : `--login-password` ou `CALLISTSEO_LOGIN_PASSWORD`).
- En-têtes refusés : `User-Agent` (fixé par l'opérateur), `Host`, `Cookie` (champ dédié) et ceux du protocole.

## Site vitrine

`site/index.html` est la page publique du projet : un seul fichier, polices embarquées, aucun script ni ressource externe, publiable tel quel (GitHub Pages, n'importe quel hébergement statique). Aperçu : `python3 -m http.server -d site`. Elle reprend les couleurs de `apps/web/src/index.css` : à garder synchronisées à la main. Les boutons « Se connecter » pointent vers `http://127.0.0.1:8080/login` (instance Docker locale) : remplacer cette adresse par celle de votre instance avant publication.

## Version web (Docker Compose)

Un seul conteneur sert l'API et l'interface (même origine, pas de CORS). Les audits sont dans un volume Docker.

```
cd deploy
cp .env.example .env            # puis renseigner CALLISTSEO_TOKEN (openssl rand -hex 24)
docker compose up -d --build    # interface et API sur http://127.0.0.1:8080
```

- **Jeton obligatoire** : sans `CALLISTSEO_TOKEN` (16 caractères minimum), Compose refuse de démarrer et le serveur aussi. C'est le mot de passe de l'équipe, demandé par l'écran de connexion (voir « Accès d'équipe »).
- **Local par défaut** : le port n'est publié que sur `127.0.0.1`. Pour l'exposer : profil `https` (Caddy, certificat automatique), `docker compose --profile https up -d` avec `CALLISTSEO_DOMAIN` et `CALLISTSEO_TRUST_PROXY=1` dans `.env`. Sans HTTPS, le jeton circule en clair.
- **Image publiée** : chaque tag `v*` publie `ghcr.io/<propriétaire>/<dépôt>` (amd64, arm64) ; `CALLISTSEO_IMAGE` dans `.env` pour l'utiliser au lieu d'un build local.
- **Protections HTTP** : CSP stricte (aucun script ni ressource externe), `nosniff`, `no-referrer`, interdiction d'affichage en cadre ; 10 tentatives de connexion ratées par minute et par adresse, puis 429 ; journal d'accès sur stderr (`docker compose logs`), sans les paramètres d'URL.
- **Durci** : utilisateur non-root, système de fichiers en lecture seule (sauf `/data`), aucune capacité Linux, `no-new-privileges`. Les destinations privées (dont les adresses internes du réseau Docker et les métadonnées cloud) sont refusées par le crawler.
- **Sonde de santé** : `GET /api/health` (publique, renvoie aussi la version, affichée dans l'en-tête de l'interface). Arrêt propre sur SIGTERM.
- **Sauvegarde** : copiez le volume `callistseo_callistseo-data` (base SQLite `audits.db`). Avant chaque migration de schéma, une copie `audits.db.avant-v<N>.bak` est écrite automatiquement (retour arrière : `docs/installation.md`).
- **Limites connues** : une seule instance (SQLite), pas de comptes utilisateurs (un jeton partagé).

## Serveur MCP (assistants IA)

`callistseo mcp --db audits.db` lance un serveur [MCP](https://modelcontextprotocol.io) sur stdio : un assistant (Claude Code, Claude Desktop, etc.) peut créer des projets, lancer des audits et interroger leurs résultats en langage naturel. Les audits tournent tant que l'assistant garde le serveur ouvert.

```
claude mcp add callistseo -- callistseo mcp --db ~/callistseo.db
```

Pour un client qui se configure en JSON :

```json
{ "mcpServers": { "callistseo": { "command": "callistseo", "args": ["mcp", "--db", "/chemin/vers/callistseo.db"] } } }
```

Outils : `list_projects`, `create_project`, `list_audits`, `start_audit`, `audit_status`, `wait_for_audit`, `stop_audit`, `audit_overview`, `query_urls`, `get_url`, `compare_audits`, `generate_sitemap`, `list_rules`.

Garde-fous : les outils passent par la même API que l'interface (filtre anti-SSRF, plafonds, `robots.txt` respecté, adresses privées refusées) ; la suppression de projet, la configuration et la planification ne sont **pas** exposées ; les résultats sont tronqués à 60 000 caractères. Les textes des pages (titres, descriptions, extractions) viennent de sites tiers : le serveur le rappelle à l'assistant, mais gardez un client qui demande confirmation avant les actions qui contactent un site.

## Application desktop (Tauri 2)

L'application embarque le moteur et l'API : l'interface appelle le backend Rust en processus, **sans ouvrir de port réseau** ni demander de jeton. Les audits sont stockés dans le dossier de données de l'application (`audits.db`).

```
pnpm install
pnpm --filter desktop dev      # développement (lance aussi le serveur Vite)
pnpm --filter desktop build    # bundle de l'application (macOS : .app / .dmg)
```

Prérequis : Rust stable, Node, pnpm, et sous Linux les dépendances système de Tauri (webkit2gtk). L'application desktop n'est pas compilée par défaut (`cargo test` l'ignore) car elle exige l'interface construite.

**Publier une version** : mettre la version dans `apps/desktop/src-tauri/tauri.conf.json`, puis pousser le tag correspondant (`git tag v0.1.0 && git push origin v0.1.0`). Le workflow `.github/workflows/release.yml` compile Windows (`.exe`, `.msi`), macOS (Apple Silicon et Intel, `.dmg`) et Linux (`.deb`, `.rpm`, `.AppImage`), et crée un **brouillon** de release avec ces fichiers : à relire, puis publier. Le workflow échoue si le tag ne correspond pas à la version. Les installateurs ne sont pas encore signés : Windows (SmartScreen) et macOS (Gatekeeper) afficheront un avertissement.

**Mise à jour** : au démarrage, l'application interroge GitHub Releases (`latest.json`) et propose la version plus récente ; rien n'est installé sans clic. C'est le seul appel sortant de l'application desktop (hors ligne, il est ignoré). Les mises à jour sont signées avec une clé Tauri (distincte de la signature Windows/macOS) : `pnpm tauri signer generate`, clé publique dans `tauri.conf.json`, clé privée et son mot de passe dans les secrets GitHub `TAURI_SIGNING_PRIVATE_KEY` et `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. Une release en brouillon ou en pré-version n'est pas proposée : publier la release pour la rendre disponible.

## Benchmarks

Un corpus synthétique déterministe (défauts injectés en nombre connu) mesure le débit, la mémoire maximale, la latence et l'exactitude des règles. Résultats et méthode : [`docs/benchmarks.md`](docs/benchmarks.md).

```
cargo build --release -p callistseo-cli -p callistseo-bench
./target/release/callistseo-bench run --out docs/benchmarks.md   # --quick : vérification rapide
./target/release/callistseo-bench serve --pages 10000 --port 8080 # même corpus pour tester un autre crawler
```

## Identification du robot

Le crawler s'annonce sur **toutes** ses requêtes (pages, `robots.txt`, sitemaps) avec l'en-tête `User-Agent` :

```
CallistSEO/<version>
```

- **`robots.txt`** : un site cible le robot avec `User-agent: CallistSEO` (casse ignorée). Seul le nom avant le `/` compte (« jeton de produit », RFC 9309) : un groupe `User-agent: seo` ne nous concerne pas.
- **Bonne pratique : ajoutez votre propre page d'information.** Un administrateur qui voit passer le robot doit pouvoir savoir qui l'exploite. Aucune URL n'est mise par défaut (une adresse fictive serait trompeuse) :

```
callistseo crawl https://exemple.fr/ --user-agent "CallistSEO/0.1 (+https://votre-site.fr/bot)"
export CALLISTSEO_USER_AGENT="CallistSEO/0.1 (+https://votre-site.fr/bot)"   # ou la variable d'environnement
```

- **Serveur web / Docker** : l'identifiant est fixé par l'opérateur (`--user-agent`, `CALLISTSEO_USER_AGENT`, voir `deploy/.env.example`) et s'applique à tous les audits ; les utilisateurs de l'API ne peuvent pas le choisir, donc pas le faire passer pour un navigateur.
- Validé au démarrage : 1 à 200 caractères ASCII imprimables.
- Si un site bloque le robot, autorisez ce nom dans sa configuration plutôt que de contourner : l'identification honnête est un principe du projet.
- **Usage responsable** : n'auditez que des sites que vous possédez ou pour lesquels vous avez l'autorisation (conditions d'utilisation, RGPD si des données personnelles sont collectées).

## Sécurité (déjà en place)

- http/https uniquement.
- IP privées, loopback, link-local et réservées refusées, après résolution DNS (anti-rebinding : la connexion utilise les adresses filtrées). `--allow-private` n'est là que pour les tests locaux.
- Redirections non suivies automatiquement : chaque cible repasse par la file (dédup + même validation).
- Limites : pages, profondeur, concurrence, délai entre requêtes, taille de réponse.
- Politesse : `Crawl-delay`, `--delay-ms`, pause sur 429/503, user-agent identifiable.

## Pas encore fait

Comptes utilisateurs et séparation des données (un seul jeton partagé), HTTPS intégré, mémoire bornée pour les très gros sites (les URL s'affichent pendant l'audit, mais l'analyse garde tout en mémoire jusqu'à la fin), extraction de hreflang / Open Graph / `X-Robots-Tag`, analyse sémantique, import Search Console, signature des applications desktop. Détails et priorités : [`ROADMAP.md`](ROADMAP.md).

## Développement

Installation détaillée : [`docs/installation.md`](docs/installation.md). Contribuer : [`CONTRIBUTING.md`](CONTRIBUTING.md). Sécurité : [`SECURITY.md`](SECURITY.md).

Un `Makefile` regroupe les commandes courantes : `make help` les liste.

```
make install     # dépendances JavaScript
make check       # tout ce que vérifie la CI : format, clippy, tests Rust, build et tests web
make serve       # API sur 127.0.0.1:8080 (avec `make web-dev` pour l'interface)
make serve-ui    # API + interface sur un seul port, comme la version Docker
make crawl URL=https://exemple.fr/
make bench-quick # benchmarks, vérification rapide
```

Sans `make` : `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`.
