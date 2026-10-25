import { describe, expect, it } from 'vitest'
import type { LiveStats } from './api'
import { formatEta, runProgress } from './progress'

const live = (o: Partial<LiveStats> = {}): LiveStats => ({
  in_flight: 8,
  concurrency: 8,
  queued: 92,
  max_pages: 0,
  urls_per_second: 10,
  avg_latency_ms: 100,
  failures: 0,
  paused: false,
  stopping: false,
  ...o,
})

describe('runProgress', () => {
  it('compte les URL terminées sur les URL découvertes', () => {
    const p = runProgress(100, live())
    expect(p).toMatchObject({ done: 100, total: 200, percent: 50, etaSeconds: 10 })
  })

  it('plafonne le total par la limite de pages', () => {
    expect(runProgress(100, live({ max_pages: 150 })).total).toBe(150)
    expect(runProgress(100, live({ max_pages: 150 })).percent).toBe(66)
  })

  it('ne dit jamais 100 % tant que l\'audit tourne', () => {
    expect(runProgress(500, live({ in_flight: 0, queued: 0 })).percent).toBe(99)
  })

  it('sans donnée ou en pause : pas de durée restante', () => {
    expect(runProgress(0, undefined)).toEqual({ done: 0, total: 0, percent: 0, etaSeconds: null })
    expect(runProgress(100, live({ paused: true })).etaSeconds).toBeNull()
    expect(runProgress(100, live({ urls_per_second: 0 })).etaSeconds).toBeNull()
  })
})

describe('formatEta', () => {
  it('formate secondes, minutes et heures', () => {
    expect(formatEta(45)).toBe('45 s')
    expect(formatEta(185)).toBe('3 min 05 s')
    expect(formatEta(3720)).toBe('1 h 02 min')
  })
})
