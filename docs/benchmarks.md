# Benchmarks CallistSEO

- Date : 2026-10-20T17:12:18Z
- Version : 0.1.0
- Machine : macos aarch64 · Apple M5 · 10 cœurs logiques · 26 Go de RAM
- Corpus : site synthétique déterministe servi en local (`callistseo-bench serve`), 4 à 5 liens par page, défauts injectés en nombre connu
- Méthode : le vrai binaire `callistseo crawl` (robots.txt respecté, profondeur 100, export CSV) tourne en processus séparé ; durée = du lancement à la fin ; mémoire = RSS maximale du processus (`wait4`). Le serveur du corpus tourne sur la même machine et lui prend un peu de CPU.

## Débit et mémoire

| Série | Corpus | Pages | Concurrence | Latence simulée | URL explorées | Durée | Débit | Mémoire max | Mémoire / 1 000 pages |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| taille | léger | 1000 | 8 | 0 ms | 1025 | 0.30 s | 3474 pages/s | 9 Mo | 8.7 Mo |
| taille | léger | 10000 | 8 | 0 ms | 10250 | 0.24 s | 42336 pages/s | 27 Mo | 2.7 Mo |
| taille | léger | 50000 | 8 | 0 ms | 51250 | 1.19 s | 43020 pages/s | 105 Mo | 2.1 Mo |
| concurrence | léger | 10000 | 1 | 0 ms | 10250 | 0.77 s | 13302 pages/s | 26 Mo | 2.6 Mo |
| concurrence | léger | 10000 | 4 | 0 ms | 10250 | 0.35 s | 28904 pages/s | 28 Mo | 2.7 Mo |
| concurrence | léger | 10000 | 16 | 0 ms | 10250 | 0.17 s | 59936 pages/s | 30 Mo | 2.9 Mo |
| concurrence | léger | 10000 | 64 | 0 ms | 10250 | 0.15 s | 69619 pages/s | 33 Mo | 3.2 Mo |
| latence réseau | léger | 2000 | 8 | 20 ms | 2050 | 5.82 s | 352 pages/s | 11 Mo | 5.6 Mo |
| latence réseau | léger | 2000 | 32 | 20 ms | 2050 | 1.58 s | 1295 pages/s | 13 Mo | 6.2 Mo |
| pages denses | dense | 1000 | 8 | 0 ms | 1025 | 0.24 s | 4250 pages/s | 74 Mo | 71.8 Mo |
| pages denses | dense | 3000 | 8 | 0 ms | 3075 | 0.69 s | 4440 pages/s | 182 Mo | 59.2 Mo |
| pages denses, latence 500 ms | dense | 400 | 8 | 500 ms | 410 | 27.83 s | 15 pages/s | 42 Mo | 102.0 Mo |
| pages denses, latence 500 ms | dense | 400 | 16 | 500 ms | 410 | 14.73 s | 28 pages/s | 41 Mo | 99.5 Mo |

## Latence d'une requête (client du crawler, une à la fois)

| Latence simulée | p50 | p95 | p99 | max |
| ---: | ---: | ---: | ---: | ---: |
| 0 ms | 0.09 ms | 0.14 ms | 0.18 ms | 0.24 ms |
| 20 ms | 22.32 ms | 22.63 ms | 23.07 ms | 23.19 ms |

## Exactitude des règles (vérité terrain exacte)

Chaque règle doit retrouver exactement le nombre de défauts injectés.

| Pages | Règle | Attendu | Trouvé | |
| ---: | --- | ---: | ---: | --- |
| 1000 | `broken-link` | 25 | 25 | ✅ |
| 1000 | `description-missing` | 100 | 100 | ✅ |
| 1000 | `h1-missing` | 34 | 34 | ✅ |
| 1000 | `http-error` | 25 | 25 | ✅ |
| 1000 | `title-duplicate` | 40 | 40 | ✅ |
| 1000 | `title-missing` | 20 | 20 | ✅ |
| 10000 | `broken-link` | 250 | 250 | ✅ |
| 10000 | `description-missing` | 1000 | 1000 | ✅ |
| 10000 | `h1-missing` | 334 | 334 | ✅ |
| 10000 | `http-error` | 250 | 250 | ✅ |
| 10000 | `title-duplicate` | 400 | 400 | ✅ |
| 10000 | `title-missing` | 200 | 200 | ✅ |
| 50000 | `broken-link` | 1250 | 1250 | ✅ |
| 50000 | `description-missing` | 5000 | 5000 | ✅ |
| 50000 | `h1-missing` | 1667 | 1667 | ✅ |
| 50000 | `http-error` | 1250 | 1250 | ✅ |
| 50000 | `title-duplicate` | 2000 | 2000 | ✅ |
| 50000 | `title-missing` | 1000 | 1000 | ✅ |
| 10000 | `broken-link` | 250 | 250 | ✅ |
| 10000 | `description-missing` | 1000 | 1000 | ✅ |
| 10000 | `h1-missing` | 334 | 334 | ✅ |
| 10000 | `http-error` | 250 | 250 | ✅ |
| 10000 | `title-duplicate` | 400 | 400 | ✅ |
| 10000 | `title-missing` | 200 | 200 | ✅ |
| 10000 | `broken-link` | 250 | 250 | ✅ |
| 10000 | `description-missing` | 1000 | 1000 | ✅ |
| 10000 | `h1-missing` | 334 | 334 | ✅ |
| 10000 | `http-error` | 250 | 250 | ✅ |
| 10000 | `title-duplicate` | 400 | 400 | ✅ |
| 10000 | `title-missing` | 200 | 200 | ✅ |
| 10000 | `broken-link` | 250 | 250 | ✅ |
| 10000 | `description-missing` | 1000 | 1000 | ✅ |
| 10000 | `h1-missing` | 334 | 334 | ✅ |
| 10000 | `http-error` | 250 | 250 | ✅ |
| 10000 | `title-duplicate` | 400 | 400 | ✅ |
| 10000 | `title-missing` | 200 | 200 | ✅ |
| 10000 | `broken-link` | 250 | 250 | ✅ |
| 10000 | `description-missing` | 1000 | 1000 | ✅ |
| 10000 | `h1-missing` | 334 | 334 | ✅ |
| 10000 | `http-error` | 250 | 250 | ✅ |
| 10000 | `title-duplicate` | 400 | 400 | ✅ |
| 10000 | `title-missing` | 200 | 200 | ✅ |
| 2000 | `broken-link` | 50 | 50 | ✅ |
| 2000 | `description-missing` | 200 | 200 | ✅ |
| 2000 | `h1-missing` | 67 | 67 | ✅ |
| 2000 | `http-error` | 50 | 50 | ✅ |
| 2000 | `title-duplicate` | 80 | 80 | ✅ |
| 2000 | `title-missing` | 40 | 40 | ✅ |
| 2000 | `broken-link` | 50 | 50 | ✅ |
| 2000 | `description-missing` | 200 | 200 | ✅ |
| 2000 | `h1-missing` | 67 | 67 | ✅ |
| 2000 | `http-error` | 50 | 50 | ✅ |
| 2000 | `title-duplicate` | 80 | 80 | ✅ |
| 2000 | `title-missing` | 40 | 40 | ✅ |
| 1000 | `broken-link` | 25 | 25 | ✅ |
| 1000 | `description-missing` | 100 | 100 | ✅ |
| 1000 | `h1-missing` | 34 | 34 | ✅ |
| 1000 | `http-error` | 25 | 25 | ✅ |
| 1000 | `title-duplicate` | 40 | 40 | ✅ |
| 1000 | `title-missing` | 20 | 20 | ✅ |
| 3000 | `broken-link` | 75 | 75 | ✅ |
| 3000 | `description-missing` | 300 | 300 | ✅ |
| 3000 | `h1-missing` | 100 | 100 | ✅ |
| 3000 | `http-error` | 75 | 75 | ✅ |
| 3000 | `title-duplicate` | 120 | 120 | ✅ |
| 3000 | `title-missing` | 60 | 60 | ✅ |
| 400 | `broken-link` | 10 | 10 | ✅ |
| 400 | `description-missing` | 40 | 40 | ✅ |
| 400 | `h1-missing` | 14 | 14 | ✅ |
| 400 | `http-error` | 10 | 10 | ✅ |
| 400 | `title-duplicate` | 16 | 16 | ✅ |
| 400 | `title-missing` | 8 | 8 | ✅ |
| 400 | `broken-link` | 10 | 10 | ✅ |
| 400 | `description-missing` | 40 | 40 | ✅ |
| 400 | `h1-missing` | 14 | 14 | ✅ |
| 400 | `http-error` | 10 | 10 | ✅ |
| 400 | `title-duplicate` | 16 | 16 | ✅ |
| 400 | `title-missing` | 8 | 8 | ✅ |

**78/78 comptes exacts (100.0 %)** — objectif du plan : ≥ 99 % sur le corpus de test.

Les comptes de la règle `deep-page` ne sont pas comparés : elle dépend de la profondeur du parcours, pas d'un défaut injecté.

## Comment lire ces chiffres

- **Débit local ≠ débit réel.** Le corpus est servi en local, avec des pages de moins de 1 Ko : le crawler est limité par son propre CPU. Sur un vrai site, le débit est limité par la latence réseau et par la politesse envers le serveur ; les lignes « latence réseau » montrent ce régime (débit ≈ concurrence ÷ latence).
- **Mémoire.** La valeur inclut quelques Mo de base du processus : la pente entre les tailles (1 000 → 10 000 → 50 000 pages) est le coût réel par page. Le crawler garde tous les résultats en mémoire jusqu'à la fin de l'audit ; extrapoler à des sites très volumineux (plusieurs centaines de milliers d'URL) demande de vérifier la limite de RAM.
- **Concurrence élevée.** Par défaut il n'y a aucun délai entre requêtes : une concurrence de 64 est rapide en local mais agressive pour un vrai serveur. Utiliser `--delay-ms` ; le `Crawl-delay` de robots.txt est appliqué automatiquement.
- **Pas de comparaison avec d'autres outils** : ces mesures ne disent rien de leur performance relative. Le corpus peut être servi à n'importe quel crawler avec `callistseo-bench serve --pages N`.

Reproduire : `cargo build --release -p callistseo-cli -p callistseo-bench && ./target/release/callistseo-bench run --out docs/benchmarks.md` (`--quick` pour une vérification rapide, code retour 1 si un compte de règle est inexact).
