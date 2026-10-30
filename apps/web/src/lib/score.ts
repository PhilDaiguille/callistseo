import type { Crawl, Severity } from './api'

export type ScoreLevel = 'good' | 'mid' | 'bad'

/** Seuils du score de santé (comme Ahrefs) : ≥ 80 bon, 50-79 moyen, < 50 mauvais. */
export function scoreLevel(score: number): ScoreLevel {
  return score >= 80 ? 'good' : score >= 50 ? 'mid' : 'bad'
}

export const SCORE_LABEL: Record<ScoreLevel, string> = { good: 'Bon', mid: 'Moyen', bad: 'Mauvais' }

/** Couleur CSS du niveau (jetons `--score-*` de index.css). */
export const scoreColor = (score: number) => `var(--score-${scoreLevel(score)})`

/** Pastels de la charte par gravité : rouge doux, ambre, lavande (`--primary`). */
export const SEVERITY_COLOR: Record<Severity, string> = { Error: 'var(--score-bad)', Warning: 'var(--score-mid)', Info: 'var(--primary)' }

/** Somme des URL concernées par gravité (une URL compte une fois par règle). */
export function severityTotals(issues: { severity: Severity; count: number }[]): Record<Severity, number> {
  const out: Record<Severity, number> = { Error: 0, Warning: 0, Info: 0 }
  for (const i of issues) out[i.severity] += i.count
  return out
}

const PERCENT = new Intl.NumberFormat('fr-FR', { style: 'percent', maximumFractionDigits: 0 })

/** Part des URL explorées touchées par un problème : « 6 % », « < 1 % » plutôt qu'un 0 % trompeur. */
export function share(count: number, pages: number): { ratio: number; text: string } {
  const ratio = pages > 0 ? Math.min(1, count / pages) : 0
  return { ratio, text: ratio > 0 && ratio < 0.005 ? `< ${PERCENT.format(0.01)}` : PERCENT.format(ratio) }
}

/** Points de la courbe de tendance : audits terminés, du plus ancien au plus récent. */
export function trendPoints(crawls: Crawl[]): { id: number; health: number; errors: number }[] {
  return crawls
    .filter((c): c is Crawl & { health: number } => c.status === 'done' && c.health !== null)
    .sort((a, b) => a.id - b.id)
    .map((c) => ({ id: c.id, health: c.health, errors: c.errors }))
}
