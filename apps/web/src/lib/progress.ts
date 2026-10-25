import type { LiveStats } from './api'

export interface RunProgress {
  /** URL terminées. */
  done: number
  /** Total connu à cet instant : terminées + en vol + en attente, plafonné par la limite de pages. */
  total: number
  /** 0-99 tant que l'audit tourne (le parcours découvre encore des URL : le total grandit). */
  percent: number
  /** Secondes restantes au rythme actuel, `null` si inconnu (pause, aucun débit). */
  etaSeconds: number | null
}

/** Avancement d'un audit en cours, comme la barre d'un crawler de bureau : terminées / découvertes. */
export function runProgress(pages: number, live: LiveStats | undefined): RunProgress {
  const found = pages + (live?.in_flight ?? 0) + (live?.queued ?? 0)
  const total = live && live.max_pages > 0 ? Math.min(found, live.max_pages) : found
  const percent = total > 0 ? Math.min(99, Math.floor((pages / total) * 100)) : 0
  const remaining = total - pages
  const rate = live?.urls_per_second ?? 0
  const etaSeconds = live && !live.paused && rate > 0 && remaining > 0 ? Math.ceil(remaining / rate) : null
  return { done: pages, total, percent, etaSeconds }
}

/** « 45 s », « 3 min 05 s », « 1 h 02 min ». */
export function formatEta(seconds: number): string {
  if (seconds < 60) return `${seconds} s`
  const m = Math.floor(seconds / 60)
  if (m < 60) return `${m} min ${String(seconds % 60).padStart(2, '0')} s`
  return `${Math.floor(m / 60)} h ${String(m % 60).padStart(2, '0')} min`
}
