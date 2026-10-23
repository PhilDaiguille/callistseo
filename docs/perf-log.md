# Journal de performance

Mesures avant/après de chaque optimisation, sur la même machine (Apple M5, 10 cœurs logiques, 26 Go) avec `callistseo-bench` (`make bench`). Les chiffres complets sont dans [`benchmarks.md`](benchmarks.md) ; ce fichier garde l'historique et la logique des décisions.

## Point de départ (avant la phase « vitesse et mémoire »)

Constat de départ : 1 000 URL de `solutions.lesechos.fr` en 1 min 24 s (≈ 11,9 pages/s). Chaque page y met 0,45 à 0,62 s à répondre (HTTP/2, 29 Ko compressés, aucun `Crawl-delay`) : avec 8 requêtes en parallèle le plafond théorique est ≈ 14,5 pages/s. **Le crawler est limité par la latence du serveur, pas par son calcul.**

| Corpus | Pages | Concurrence | Latence simulée | Débit | Mémoire max | Mémoire / 1 000 pages |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| léger (4 liens, < 1 Ko) | 10 000 | 8 | 0 ms | 41 545 pages/s | 30 Mo | 2,9 Mo |
| léger | 50 000 | 8 | 0 ms | 42 923 pages/s | 115 Mo | 2,2 Mo |
| dense (≈ 250 Ko, ≈ 300 liens) | 1 000 | 8 | 0 ms | 4 242 pages/s | 88 Mo | 85,4 Mo |
| dense | 3 000 | 8 | 0 ms | 4 341 pages/s | 224 Mo | 72,8 Mo |
| dense | 400 | 8 | 500 ms | 15 pages/s | 45 Mo | 109,3 Mo |
| dense | 400 | 16 | 500 ms | 28 pages/s | 48 Mo | 117,5 Mo |

Enseignements :
- **Mémoire** : la mémoire conservée par page dense est d'environ **70 Ko** (pente entre 1 000 et 3 000 pages), soit ≈ 7 Go pour 100 000 pages. Le corpus léger (≈ 2 Ko/page) la masquait. Cause principale : chaque lien est un `Url` possédé (≈ 225 o) dans `CrawledPage.links`.
- **Débit réseau** : à latence fixe, le débit est proportionnel à la concurrence (15 → 28 pages/s en passant de 8 à 16) : c'est le levier pour les sites lents.

## Résultats de la phase 1

| Changement | Mesure | Décision |
| --- | --- | --- |
| Internement des URL (`Arc<Url>`, `UrlInterner`) | une allocation par URL distincte au lieu d'une par lien | adopté |
| Piège du `collect` en place (`Vec<Url>` → `Vec<Arc<Url>>` réutilisait le tampon de 88 o/élément) | mémoire, corpus dense 6 000 pages : **346 Mo → 49 Mo** (≈ 3,5 Ko/page, contre 54–70 Ko) | corrigé (`with_capacity` + `extend`), test de non-régression |
| Sélecteurs précompilés (`LazyLock`) | supprime 5 `Selector::parse` par page | adopté |
| Comparaison d'hôte sans allocation | — | adopté |
| `scraper` → `lol_html` | analyse ≈ 1 ms par page dense, soit ≈ 0,2 % de la latence | **rejeté pour l'instant** (porte de décision non franchie) |
| `mimalloc` | consomme plus de mémoire | **rejeté** |
| LTO fat + `codegen-units=1` | ≈ 3 % de CPU, build 81 s contre 46 s | **thin LTO** retenu |
| Concurrence adaptative (AIMD, 8 → 16) | à latence 500 ms : 15 pages/s à C=8, 28 pages/s à C=16 | adopté ; un `--delay-ms` / `Crawl-delay` fige la concurrence |

Le débit est limité par la latence du serveur (débit ≈ concurrence ÷ latence), pas par le CPU : le levier est la concurrence, pas la réécriture de l'analyse.

Reste pour clore la phase : test d'endurance (RSS stable après N audits), scénario « adaptatif » dans le banc, `cargo audit`/`cargo deny` (outils à installer, avec votre accord).

## Clôture de la phase 1

- **Endurance** (`callistseo-bench soak --runs 8 --pages 400`, pages denses) : pic mémoire 32,1 → 35,5 Mo dès le 2ᵉ audit, puis 36,0 Mo au 8ᵉ. Plateau : pas de fuite.
- **Adaptatif** (1 500 pages légères, latence 100 ms) : concurrence fixe 8 → 19,6 s (≈ 77 pages/s) ; adaptative (8 → 16) → 10,3 s (≈ 146 pages/s).
- **`cargo audit`** : 0 vulnérabilité ; 3 avertissements indirects (`fxhash`, `proc-macro-error`, `glib` 0.18), venus de Tauri et `scraper`.
- Mesure réelle (solutions.lesechos.fr, sur 1 000 URL) : 13,2 URL/s à 956 ms et concurrence 13, contre 11,9 avant ; le serveur reste le facteur limitant.

## Phase 2 : coût des faits par URL

Corpus dense, 6 000 pages : pic mémoire 49 Mo → 64 Mo (≈ +2,5 Ko par page : en-têtes, empreinte, chaînes de `PageFacts`), temps 1,3 s. Les listes plafonnées (images 100, H2 30, JSON-LD 10 × 20 Ko) bornent le pire cas ; pour 100 000 pages comptez ≈ 1 Go.

## Lenteur de l'API et de l'interface (mesurée sur l'audit dense de 3 075 pages, base de 87 Mo)

| Requête | Avant | Après | Changement |
| --- | --- | --- | --- |
| `rows` avec colonne liens entrants | 460 ms | 1,5 ms | `pages.inlinks` / `outlinks` précalculés à l'enregistrement (migration v8, audits existants recalculés en 1,5 s) |
| `rows` triées par liens entrants | 1 050 ms | 0,9 ms | idem + index `(run_id, inlinks)` |
| `links` (maillage) | 2 590 ms | 1,9 ms | statistiques lues dans les compteurs, plus aucune jointure sur `links` |
| `graph` (calcul) | 1 540 ms | ≈ 25 ms | nœuds par une requête légère (sans les colonnes volumineuses) ; arêtes sans `ORDER BY l.rowid`, qui triait 900 000 liens avant le `LIMIT` |
| `graph` (octets) | 4,37 Mo | 165 Ko gzip / 113 Ko brotli | compression |
| JS de l'interface | 476 Ko | 146 Ko gzip | compression + `Cache-Control: immutable` sur `/assets/*` (rechargements : 0 octet) |

Compression gzip et brotli selon `Accept-Encoding` (`http_app`, serveur HTTP seulement : l'appel en processus du desktop n'en a pas besoin). Le crawler négociait déjà `gzip, deflate, br` avec les sites audités (vérifié : `accept-encoding=gzip,deflate,br`).

Défaut évité : un fichier `/assets/…` absent retombe sur `index.html` avec un 200 ; il ne doit pas recevoir le cache d'un an (test dédié).

## Écriture d'un audit (`save_run`) : 3 000 pages, 900 000 liens

Mesure : `cargo test -p callistseo-storage --release perf_save_run -- --ignored --nocapture` (deux audits d'affilée dans la même base).

| | 1er audit (15 000 issues) | 2e audit (180 000 issues) |
|---|---|---|
| Avant | 1,64 s | 2,90 s |
| Après | 0,94 s | 1,93 s |

- Le nombre de règles n'est pas en cause : 12 fois plus d'issues coûtent 0,2 s en écriture.
- Le coût venait de l'`UPDATE` des liens entrants/sortants (0,75 s puis 1,3 s : il grossissait avec l'historique des audits). Il est maintenant calculé en mémoire à partir des pages déjà en main.
- Insertion des liens (≈ 0,8 à 1,2 s) : l'insertion par lots multi-lignes est **plus lente** (2,3 s) que l'`execute` ligne à ligne, abandonnée. L'index `idx_links_target` coûte à lui seul la moitié (0,9 s sur 1,5 s) mais sert à la liste des liens entrants de l'interface : conservé.
- `analyze` : 85 ms pour 50 000 pages (145 000 issues, `cargo test -p callistseo-seo-rules --release perf_analyze -- --ignored --nocapture`). Pas de `rayon` : rien à gagner.
