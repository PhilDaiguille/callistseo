# Contribuer à CallistSEO

Le code, les commentaires, l'interface et la documentation sont **en français** : gardez cette langue.

## Démarrer

Prérequis : Rust stable, Node, pnpm (Docker pour la version web).

```
make install     # dépendances JavaScript
make check       # ce que vérifie la CI : fmt, clippy -D warnings, tests Rust, build + tests web
make serve       # API sur 127.0.0.1:8080  (avec `make web-dev` pour l'interface)
```

`make help` liste toutes les commandes. Un test précis : `cargo test -p callistseo-crawler robots`.
L'app desktop n'est pas dans `cargo test` (elle exige `apps/web/dist`) : `make web-build` puis `cargo build -p callistseo-desktop`.

## Avant d'ouvrir une pull request

1. `make check` passe.
2. Un comportement modifié ou ajouté a un test (les règles : un test de `seo-rules` ; le crawler : un test d'intégration dans `crawler/tests/crawl.rs`).
3. [`ROADMAP.md`](ROADMAP.md) et [`README.md`](README.md) sont à jour si le comportement change.
4. Une PR = un sujet, avec une description de ce qui change et pourquoi.

## Conventions à connaître

- **Ajouter une règle SEO** : entrée dans la table `RULES` de `crates/seo-rules/src/info.rs` (gravité : Erreur = cassé, Avertissement = à corriger, Info = à vérifier), logique dans `seo-rules`, et **le même texte dans `apps/web/src/lib/rules.ts`** (synchronisé à la main).
- **Ajouter un fait par URL** : champ dans `core::PageFacts` + **nouvelle** migration (jamais modifier une migration existante) + `PAGE_COLS` / `read_facts` / `INSERT` du stockage.
- **SQL** : aucune valeur utilisateur dans une requête ; rapports et colonnes passent par les listes blanches de `storage/src/reports.rs`.
- **Sécurité du crawler** : ne jamais exposer `allow_private` par l'API ; les redirections ne sont pas suivies en place.
- **Composants `components/ui/`** : adaptés dans le dépôt, ne lancez pas `shadcn add` par-dessus.
- Les erreurs Rust sont des `String` ; pas de nouvelle dépendance pour ce que quelques lignes font.
- **Mises à jour des dépendances** : Renovate (`renovate.json`), au plus une fois toutes les 2 h, versions publiées depuis 3 jours au moins, sans limite de nombre de PR. Une PR groupée pour les mineures et correctifs, fusionnée seule si la CI passe ; une PR groupée pour les majeures, à relire et fusionner à la main.

Détail de l'architecture : [`CLAUDE.md`](CLAUDE.md). Failles de sécurité : voir [`SECURITY.md`](SECURITY.md), pas d'issue publique.
