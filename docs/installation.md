# Installation

Trois façons d'utiliser CallistSEO. Aucune release publiée pour l'instant : on part du code source.

## 1. Ligne de commande

Prérequis : Rust stable.

```
cargo build --release -p callistseo-cli
./target/release/callistseo crawl https://exemple.fr/ --max-pages 200
./target/release/callistseo rules        # liste des règles et gravités
```

Code retour : 0 (aucune erreur), 1 (erreurs 4xx/5xx/réseau), 2 (appel invalide). Historique et comparaison : `--db audits.db --project nom`, puis `callistseo runs`, `export`, `diff`.

## 2. Application desktop (Tauri 2)

Prérequis : Rust stable, Node, pnpm ; sous Linux, les dépendances système de Tauri (webkit2gtk).

```
pnpm install
pnpm --filter desktop dev       # développement
pnpm --filter desktop build     # bundle (.app/.dmg, .exe/.msi, .deb/.AppImage)
```

Aucun port réseau, aucun jeton : les audits sont dans le dossier de données de l'application (`audits.db`). Les installateurs ne sont pas signés : SmartScreen (Windows) et Gatekeeper (macOS) afficheront un avertissement. Seul macOS a été testé.

## 3. Version web (Docker Compose)

Prérequis : Docker avec Compose.

```
cd deploy
cp .env.example .env            # renseigner CALLISTSEO_TOKEN (openssl rand -hex 24, 16 caractères minimum)
docker compose up -d --build    # http://127.0.0.1:8080
```

- Le jeton est le mot de passe partagé de l'équipe (écran de connexion).
- Le port n'est publié que sur `127.0.0.1`. Pour l'exposer, utilisez le **proxy HTTPS** fourni (en HTTP le jeton circule en clair) : dans `.env`, `CALLISTSEO_DOMAIN=seo.votre-site.fr` et `CALLISTSEO_TRUST_PROXY=1`, DNS pointé sur la machine, ports 80/443 ouverts, puis `docker compose --profile https up -d`. Caddy obtient et renouvelle le certificat.
- Image publiée (amd64 et arm64, à chaque version) : `CALLISTSEO_IMAGE=ghcr.io/<propriétaire>/callistseo:latest` dans `.env`, puis `docker compose pull && docker compose up -d` (sans `--build`).
- Protections : CSP stricte sur l'interface, 10 tentatives de connexion ratées par minute et par adresse (puis 429), journal d'accès sur la sortie d'erreur (`docker compose logs callistseo` : adresse, méthode, chemin sans paramètres, statut, durée).
- Sauvegarde : copiez le volume `callistseo_callistseo-data` (base SQLite).
- Mise à jour : `git pull && docker compose up -d --build`, ou `docker compose pull && docker compose up -d` avec l'image publiée (les migrations de la base sont automatiques et ne suppriment rien).

## Dépannage

- `cargo test` ne recompile pas le binaire : refaites `cargo build -p callistseo-cli` avant de tester l'API à la main.
- Compose refuse de démarrer : `CALLISTSEO_TOKEN` absent ou trop court.
- Un site local est refusé : voulu (anti-SSRF) ; `--allow-private` n'existe qu'en CLI.
