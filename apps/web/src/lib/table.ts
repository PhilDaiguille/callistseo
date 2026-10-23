import type { Cell } from './api'

const WIDTHS: Record<string, number> = {
  url: 420,
  title: 260,
  description: 260,
  h1: 220,
  canonical: 260,
  redirect_to: 260,
  error: 220,
  content_type: 160,
  content_hash: 150,
  robots: 150,
}

/** Largeur d'une colonne, en pixels (120 par défaut). */
export const widthOf = (id: string): number => WIDTHS[id] ?? 120

/**
 * Modèle de colonnes de la grille : l'URL prend l'espace libre (largeur minimale `widthOf`),
 * les autres colonnes gardent leur largeur. Sans colonne url, la dernière colonne absorbe le reste.
 */
export function columnTemplate(columns: string[]): string {
  const flex = columns.includes('url') ? 'url' : columns.at(-1)
  return columns.map((c) => (c === flex ? `minmax(${widthOf(c)}px, 1fr)` : `${widthOf(c)}px`)).join(' ')
}

export function formatBytes(n: number): string {
  if (n < 1024) return `${n} o`
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1).replace('.', ',')} Ko`
  return `${(n / 1024 / 1024).toFixed(1).replace('.', ',')} Mo`
}

/** Texte affiché pour une cellule ; `null` s'affiche « — ». */
export function formatCell(id: string, v: Cell): string {
  if (v === null || v === '') return '—'
  if (id === 'in_sitemap') return v === 1 ? 'Oui' : 'Non'
  if (typeof v === 'number') {
    if (id === 'size') return formatBytes(v)
    if (id.endsWith('_ms')) return `${v.toLocaleString('fr-FR')} ms`
    return id === 'status' ? String(v) : v.toLocaleString('fr-FR')
  }
  return v
}

/** Minuscules sans accents : « Écran » et « ecran » se trouvent mutuellement. */
export const fold = (s: string): string => s.normalize('NFD').replace(/\p{M}/gu, '').toLowerCase()

/** Tous les mots de la recherche doivent apparaître (dans n'importe quel ordre). */
export function matches(query: string, text: string): boolean {
  const hay = fold(text)
  return fold(query)
    .split(/\s+/)
    .filter(Boolean)
    .every((w) => hay.includes(w))
}

/** Index voisin avec bouclage : utilisé par la palette et le tableau (flèches du clavier). */
export function wrap(len: number, current: number, delta: number): number {
  if (len <= 0) return -1
  return (((current + delta) % len) + len) % len
}

/** Colonnes demandées dans l'URL (`cols=a,b`), sinon celles du rapport ; les inconnues sont écartées. */
export function pickColumns(param: string | null, fallback: string[], known: string[]): string[] {
  const asked = (param ?? '').split(',').filter((c) => known.includes(c))
  return asked.length > 0 ? asked : fallback
}

const COLUMNS_KEY = 'callistseo.columns.'

/** Dernier choix de colonnes pour un rapport (confort : l'absence de stockage ne gêne rien). */
export function loadColumns(report: string): string | null {
  try {
    return localStorage.getItem(COLUMNS_KEY + report)
  } catch {
    return null
  }
}

export function saveColumns(report: string, columns: string[]): void {
  try {
    localStorage.setItem(COLUMNS_KEY + report, columns.join(','))
  } catch {
    // stockage indisponible (navigation privée) : le choix reste dans l'adresse
  }
}
