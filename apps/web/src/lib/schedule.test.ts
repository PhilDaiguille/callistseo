import { describe, expect, it } from 'vitest'
import { intervalLabel, untilLabel } from './schedule'

describe('schedule labels', () => {
  it('names intervals, including unusual ones', () => {
    expect(intervalLabel(1440)).toBe('Tous les jours')
    expect(intervalLabel(2880)).toBe('Tous les 2 jours')
    expect(intervalLabel(180)).toBe('Toutes les 3 heures')
    expect(intervalLabel(75)).toBe('Toutes les 75 minutes')
  })
  it('describes the next run', () => {
    expect(untilLabel(1000, 990)).toBe('imminent')
    expect(untilLabel(1000 + 30 * 60, 1000)).toBe('dans 30 min')
    expect(untilLabel(1000 + 5 * 3600, 1000)).toBe('dans 5 h')
    expect(untilLabel(1000 + 4 * 86400, 1000)).toBe('dans 4 jours')
  })
})
