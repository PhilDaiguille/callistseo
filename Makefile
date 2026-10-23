# CallistSEO : commandes courantes. `make` (ou `make help`) liste les cibles.
# Prérequis : Rust stable, Node, pnpm ; Docker pour les cibles docker-*.

.DEFAULT_GOAL := help
.PHONY: help install build test lint fmt fmt-check check web-dev web-build web-test \
	serve serve-ui crawl rules desktop-dev desktop-build docker-build docker-up docker-down docker-logs \
	bench bench-quick clean

# Réglages (surchargeables : `make serve DB=/tmp/a.db ADDR=127.0.0.1:9000`)
DB   ?= audits.db
ADDR ?= 127.0.0.1:8080
URL  ?=

help: ## Affiche cette aide
	@awk 'BEGIN {FS = ":.*## "} /^[a-zA-Z0-9_-]+:.*## / {printf "  \033[1m%-15s\033[0m %s\n", $$1, $$2}' $(MAKEFILE_LIST)

# --- Dépendances -------------------------------------------------------------------------------
install: ## Installe les dépendances JavaScript (pnpm)
	pnpm install

# --- Rust --------------------------------------------------------------------------------------
build: ## Compile la CLI en release (target/release/callistseo)
	cargo build --release -p callistseo-cli

test: ## Lance les tests Rust
	cargo test

lint: ## clippy, avertissements = erreurs
	cargo clippy --all-targets -- -D warnings

fmt: ## Formate le code Rust
	cargo fmt --all

fmt-check: ## Vérifie le formatage sans modifier
	cargo fmt --all --check

# --- Interface web -----------------------------------------------------------------------------
web-dev: ## Interface en développement (http://localhost:5173, proxy /api -> 127.0.0.1:8080)
	pnpm --filter web dev

web-build: ## Construit l'interface (apps/web/dist)
	pnpm --filter web build

web-test: ## Tests de l'interface
	pnpm --filter web test

check: fmt-check lint test web-build web-test ## Tout ce que vérifie la CI (Rust + web)

# --- Utilisation -------------------------------------------------------------------------------
serve: ## API seule (réglages ADDR et DB), à utiliser avec `make web-dev`
	cargo run -p callistseo-cli -- serve --db $(DB) --addr $(ADDR)

serve-ui: web-build ## API + interface construite sur un seul port, comme la version Docker
	cargo run -p callistseo-cli -- serve --db $(DB) --addr $(ADDR) --static-dir apps/web/dist

crawl: ## Audite un site et l'enregistre : make crawl URL=https://exemple.fr/
	@test -n "$(URL)" || (echo "Précisez l'URL : make crawl URL=https://exemple.fr/" >&2; exit 2)
	cargo run --release -p callistseo-cli -- crawl $(URL) --db $(DB)

rules: ## Liste les règles d'analyse avec leur gravité
	cargo run -q -p callistseo-cli -- rules

# --- Application desktop (Tauri) ---------------------------------------------------------------
desktop-dev: ## Application desktop en développement
	pnpm --filter desktop dev

desktop-build: ## Construit l'application desktop (bundle)
	pnpm --filter desktop build

# --- Docker ------------------------------------------------------------------------------------
docker-build: ## Construit l'image (deploy/Dockerfile)
	docker build -f deploy/Dockerfile -t callistseo:local .

docker-up: ## Lance la version web (jeton requis : deploy/.env, voir deploy/.env.example)
	cd deploy && docker compose up -d --build

docker-down: ## Arrête la version web (les données du volume sont conservées)
	cd deploy && docker compose down

docker-logs: ## Suit les journaux du conteneur
	cd deploy && docker compose logs -f

# --- Benchmarks --------------------------------------------------------------------------------
bench: ## Suite complète, écrit docs/benchmarks.md (quelques dizaines de secondes)
	cargo build --release -p callistseo-cli -p callistseo-bench
	./target/release/callistseo-bench run --out docs/benchmarks.md

bench-quick: ## Suite réduite (échoue si une règle n'est pas exacte)
	cargo build --release -p callistseo-cli -p callistseo-bench
	./target/release/callistseo-bench run --quick

# --- Nettoyage ---------------------------------------------------------------------------------
clean: ## Supprime target/ et apps/web/dist
	cargo clean
	rm -rf apps/web/dist
