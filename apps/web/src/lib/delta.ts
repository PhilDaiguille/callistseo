import type { Crawl } from './api'

export interface Delta {
  text: string
  /** `worse` : plus d'erreurs ; `better` : moins ; `same` : identique. */
  tone: 'worse' | 'better' | 'same'
}

/** Audit terminé le plus récent avant `current` dans la liste d'audits du projet. */
export function previousDone(current: Crawl, crawls: Crawl[]): Crawl | null {
  return (
    crawls
      .filter((c) => c.status === 'done' && c.id < current.id)
      .sort((a, b) => b.id - a.id)[0] ?? null
  )
}

/** Écart de score de santé avec l'audit précédent (`null` si l'un des deux n'a pas de score). */
export function healthDelta(current: Crawl, previous: Crawl | null): number | null {
  return current.health === null || previous?.health == null ? null : current.health - previous.health
}

const plural = (n: number, one: string, many: string) => (Math.abs(n) > 1 ? many : one)

/** « +3 erreurs depuis l'audit #2 » : l'évolution des erreurs, ce qui compte pour une CI. */
export function errorDelta(current: Crawl, previous: Crawl): Delta {
  const diff = current.errors - previous.errors
  if (diff === 0) return { text: `erreurs inchangées depuis l’audit #${previous.id}`, tone: 'same' }
  const sign = diff > 0 ? '+' : '−'
  return {
    text: `${sign}${Math.abs(diff)} ${plural(diff, 'erreur', 'erreurs')} depuis l’audit #${previous.id}`,
    tone: diff > 0 ? 'worse' : 'better',
  }
}
