export const INTERVALS: { minutes: number; label: string }[] = [
  { minutes: 60, label: 'Toutes les heures' },
  { minutes: 60 * 6, label: 'Toutes les 6 heures' },
  { minutes: 60 * 24, label: 'Tous les jours' },
  { minutes: 60 * 24 * 7, label: 'Toutes les semaines' },
  { minutes: 60 * 24 * 30, label: 'Tous les mois' },
]

/** Libellé d'un intervalle, y compris ceux qui ne sont pas dans la liste (créés par l'API). */
export function intervalLabel(minutes: number): string {
  const known = INTERVALS.find((i) => i.minutes === minutes)
  if (known) return known.label
  if (minutes % 1440 === 0) return `Tous les ${minutes / 1440} jours`
  if (minutes % 60 === 0) return `Toutes les ${minutes / 60} heures`
  return `Toutes les ${minutes} minutes`
}

/** « dans 3 h », « dans 2 jours », « imminent » : repère approximatif pour l'échéance. */
export function untilLabel(target: number, now = Date.now() / 1000): string {
  const secs = target - now
  if (secs <= 60) return 'imminent'
  const mins = Math.round(secs / 60)
  if (mins < 60) return `dans ${mins} min`
  const hours = Math.round(mins / 60)
  if (hours < 48) return `dans ${hours} h`
  return `dans ${Math.round(hours / 24)} jours`
}
