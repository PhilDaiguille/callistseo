export function formatDate(secs: number): string {
  return new Date(secs * 1000).toLocaleString('fr-FR', { dateStyle: 'medium', timeStyle: 'short' })
}

/** « 12 s », « 3 min 04 s » ; `end` absent = encore en cours (on compte jusqu'à `now`). */
export function formatDuration(start: number, end: number | null, now = Date.now() / 1000): string {
  const total = Math.max(0, Math.round((end ?? now) - start))
  if (total < 60) return `${total} s`
  return `${Math.floor(total / 60)} min ${String(total % 60).padStart(2, '0')} s`
}

export type Tone = 'ok' | 'redirect' | 'error' | 'none'

export function statusTone(status: number | null): Tone {
  if (status === null) return 'none'
  if (status >= 400) return 'error'
  if (status >= 300) return 'redirect'
  return 'ok'
}

export const PAGE_SIZE = 50

/** Page 1-indexée -> décalage de l'API ; toute valeur invalide donne la page 1. */
export function offsetFor(page: string | null, size = PAGE_SIZE): number {
  const n = Number.parseInt(page ?? '', 10)
  return Number.isFinite(n) && n > 1 ? (n - 1) * size : 0
}
