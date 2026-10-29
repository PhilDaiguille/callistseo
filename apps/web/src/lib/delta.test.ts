import { describe, expect, it } from 'vitest'
import type { Crawl } from './api'
import { errorDelta, healthDelta, previousDone } from './delta'

const crawl = (id: number, errors: number, status: Crawl['status'] = 'done'): Crawl => ({
  id, project_id: 1, project: 'p', start_url: 'http://a.test/', status, started_at: id, finished_at: id, error: null, pages: 10, errors, warnings: 0, health: 100 - errors,
})

describe('delta', () => {
  it('finds the closest earlier finished audit', () => {
    const list = [crawl(5, 0, 'running'), crawl(4, 2, 'failed'), crawl(3, 7), crawl(1, 9), crawl(6, 1)]
    expect(previousDone(crawl(6, 1), list)?.id).toBe(3)
    expect(previousDone(crawl(1, 9), list)).toBeNull()
  })
  it('describes the change in errors', () => {
    expect(errorDelta(crawl(2, 5), crawl(1, 2))).toEqual({ text: '+3 erreurs depuis l’audit #1', tone: 'worse' })
    expect(errorDelta(crawl(2, 1), crawl(1, 2))).toEqual({ text: '−1 erreur depuis l’audit #1', tone: 'better' })
    expect(errorDelta(crawl(2, 2), crawl(1, 2)).tone).toBe('same')
  })
  it('compares health scores only when both exist', () => {
    expect(healthDelta(crawl(2, 5), crawl(1, 2))).toBe(-3)
    expect(healthDelta(crawl(2, 5), null)).toBeNull()
    expect(healthDelta({ ...crawl(2, 5), health: null }, crawl(1, 2))).toBeNull()
  })
})
