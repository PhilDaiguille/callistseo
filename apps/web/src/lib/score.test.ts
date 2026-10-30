import { describe, expect, it } from 'vitest'
import type { Crawl } from './api'
import { scoreLevel, severityTotals, share, trendPoints } from './score'

const crawl = (id: number, health: number | null, status: Crawl['status'] = 'done'): Crawl => ({
  id, project_id: 1, project: 'p', start_url: 'http://a.test/', status, started_at: id, finished_at: id, error: null, pages: 10, errors: 1, warnings: 0, health,
})

describe('score', () => {
  it('maps scores to levels at the thresholds', () => {
    expect([100, 80, 79, 50, 49, 0].map(scoreLevel)).toEqual(['good', 'good', 'mid', 'mid', 'bad', 'bad'])
  })
  it('sums affected URLs by severity', () => {
    expect(severityTotals([{ severity: 'Error', count: 2 }, { severity: 'Error', count: 3 }, { severity: 'Info', count: 1 }])).toEqual({ Error: 5, Warning: 0, Info: 1 })
  })
  it('formats the share of affected URLs', () => {
    expect(share(124, 1946).text.replace(/\s/g, ' ')).toBe('6 %')
    expect(share(1, 1946).text.replace(/\s/g, ' ')).toBe('< 1 %')
    expect(share(0, 1946).ratio).toBe(0)
    expect(share(5, 0).ratio).toBe(0)
    expect(share(9, 4).ratio).toBe(1)
  })
  it('keeps finished audits with a score, oldest first', () => {
    const points = trendPoints([crawl(3, 90), crawl(4, null, 'running'), crawl(1, 70), crawl(2, null, 'failed')])
    expect(points.map((p) => p.id)).toEqual([1, 3])
  })
})
