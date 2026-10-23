# Système de design

Identité inspirée de Skymo (chaleureuse, ronde, « objets posés »), adaptée à un outil de données : le style marqué est réservé aux actions, le reste reste léger pour que les tableaux denses restent lisibles. Tout vit dans `apps/web/src/index.css` (jetons et classes communes) et `apps/web/src/components/ui/` (composants shadcn adaptés).

## Hiérarchie : fort, léger, neutre

| Niveau | Pour | Style |
| --- | --- | --- |
| **Fort** | boutons principaux/secondaires/destructifs, champs, onglet actif, cartes mises en avant (cartes projet) | contour encre 2 px + ombre décalée de 2–3 px, sans flou |
| **Léger** | panneaux, tableaux, listes, badges, blocs de chiffres | contour fin `--line`, aucune ombre |
| **Neutre** | boutons `outline`/`ghost`/`link`, texte | aucun contour fort |

Règles : un seul bouton primaire (lavande) par écran, pour l'action principale ; les couleurs fortes (rouge) sont réservées aux vrais problèmes ; ne pas mettre de contour épais sur chaque panneau ou chaque cellule (c'est ce qui rend l'ensemble lourd).

## Jetons

| Jeton | Clair | Sombre | Rôle |
| --- | --- | --- | --- |
| `--background` | `#FAF7EF` crème | `#1B1A17` | fond (avec une texture de points discrète) |
| `--foreground` / `--ink` | `#1D1C19` encre | `#F3EFE4` / `#EDE7D8` | texte / contours forts |
| `--primary` | `#C5CEEE` lavande | idem | action principale (texte encre dans les deux thèmes) |
| `--accent` | `#E8E5F2` lilas | `#3A3650` | survol, sélection |
| `--secondary` | `#E8E2D6` sable | `#34312B` | surfaces secondaires (en-têtes de tableau, liste d'onglets) |
| `--success` | `#83A58B` vert doux | idem | succès (pastilles : `--success-soft`) |
| `--destructive` | `#B3261E` | `#FF8A80` | erreurs (pastilles : `--destructive-soft`) |
| `--shadow-color` | encre | `#6F78A8` | ombres décalées (une ombre noire serait invisible en sombre) |

Rayons : `--radius` 14 px (boutons/champs `xl`, panneaux `2xl`). Typographie : **Nunito** (titres, marque, chiffres clés) et **Inter** (interface), toutes deux embarquées via `@fontsource-variable` (pas de CDN : l'app desktop impose `font-src 'self'` et Docker doit marcher hors ligne).

## Classes et composants

- `.panel` (léger), `.table-panel` (panneau qui contient un `<Table>`), `.panel-pop` (fort), `.field` (`<select>` natifs alignés sur `Input`) ; `Card` a une variante `variant="pop"`.
- `Button` : `default` (lavande), `secondary` (crème), `destructive` portent le contour + l'ombre ; `outline`/`ghost`/`link` sont à plat. Tailles : 40 px par défaut, `sm`/`xs` pour les barres d'outils de tableaux.
- `Badge` : `success` / `warning` / `info` / `destructive` / `outline` (pastel, sans contour fort). Gravités : Erreur = rouge doux, Avertissement = ambre, Info = lilas ; statuts HTTP 2xx = vert, 3xx = ambre, 4xx/5xx = rouge.
- `Stat` (chiffre clé), `EmptyState` (état vide avec la mascotte), `Mascot` (SVG, humeurs `normal` / `happy` / `sleepy`).
- `UrlLink` : **toute URL affichée passe par lui** (texte `text-sm` mono tronqué avec l'URL entière en infobulle, lien vers la fiche, icône « ouvrir dans un nouvel onglet » `rel="noopener noreferrer"`, bouton copier). `compact` (cellules du tableau) : actions au survol et hors tabulation ; `short` (volet étroit) : chemin seul. En desktop l'ouverture passe par la commande Tauri `open_url` (http/https seulement). `LinkList` : liens entrants/sortants (statut, filtre, « cassés seulement », copie de la liste).
- Les composants de `components/ui/` ont été **modifiés dans le dépôt** : un `shadcn add` les écraserait.

## Thème

Clair, sombre ou « auto » (suit le système). La préférence est mémorisée (`localStorage`) et `?theme=dark|light` permet de partager/tester un thème. Logique pure et testée : `src/lib/theme.ts`.

## Accessibilité et responsive

- `src/lib/contrast.test.ts` lit les couleurs de `index.css` et exige 4,5:1 (WCAG AA) sur les paires texte/fond utilisées, en clair et en sombre : modifier la palette sans respecter le contraste fait échouer le test.
- Focus visible : contour 3 px (`--ring`) sur tous les éléments. Animations sous `motion-safe` / désactivées si `prefers-reduced-motion`.
- Une grille sans colonnes explicites est contrainte à `minmax(0, 1fr)` (règle de base de `index.css`) : sans cela, un onglet ou un tableau large fait déborder la page sur mobile.

## Le rapport HTML autonome

`crates/export` (`REPORT_CSS`) reprend la palette et les rayons, sans police externe ni script (il doit rester un seul fichier portable).

## Ce qui n'est pas fait

Pas de maquettes Figma, pas de guide d'illustration au-delà de la mascotte, mascotte à faire retoucher par un illustrateur si l'identité doit durer, rendu mobile et thème sombre vérifiés par captures seulement (pas sur appareil réel, pas de lecteur d'écran).

## Repères d'interaction (ajoutés avec le poste de travail)

- **Poste de travail pleine place** : en `lg`, la route d'un audit est une colonne de la hauteur de la fenêtre (`Layout` : `h-svh`, `main` en `flex-1`), sans plafond de largeur ; le tableau prend le reste et la colonne `url` absorbe la largeur libre (`columnTemplate`).
- **Chargement** : `Skeleton` / `PageSkeleton` plutôt qu'un texte « Chargement… » (la page ne saute pas).
- **Titre d'onglet** par page (`useDocumentTitle`) ; lien d'évitement « Aller au contenu » en tête de page.
- **Résumé d'un audit** : pastilles cliquables (erreurs → « à corriger en premier ») et évolution par rapport à l'audit précédent terminé (`lib/delta.ts`), en pastille teintée plutôt qu'en texte coloré (contraste en thème clair).
- **Page projet** : onglets Audits / Planification / Configuration (`?tab=`), l'historique d'abord.
- **Accueil** : chaque projet montre l'état de son dernier audit (`last_crawl`) ; le formulaire de création est replié dès qu'il existe un projet.
- **Tableau** : les colonnes choisies sont mémorisées par rapport (`localStorage`, confort seulement : l'adresse prime).
