import { describe, expect, it } from 'vitest'
import { formatDuration, offsetFor, statusTone } from './format'
import { qs } from './api'
import { RULES, ruleInfo } from './rules'

describe('format', () => {
  it('formats durations', () => {
    expect(formatDuration(100, 112)).toBe('12 s')
    expect(formatDuration(0, 184)).toBe('3 min 04 s')
    expect(formatDuration(100, null, 130)).toBe('30 s')
    expect(formatDuration(100, 50)).toBe('0 s')
  })

  it('classifies HTTP statuses', () => {
    expect([null, 200, 301, 404, 503].map(statusTone)).toEqual(['none', 'ok', 'redirect', 'error', 'error'])
  })

  it('converts page numbers to offsets and ignores bad input', () => {
    expect([null, 'x', '0', '-3', '1', '2', '3'].map((p) => offsetFor(p))).toEqual([0, 0, 0, 0, 0, 50, 100])
  })
})

describe('api helpers', () => {
  it('builds query strings without empty values', () => {
    expect(qs({})).toBe('')
    expect(qs({ a: '', b: null, c: undefined })).toBe('')
    expect(qs({ search: 'a b&c', limit: 50, offset: 0 })).toBe('?search=a+b%26c&limit=50&offset=0')
  })
})

describe('rules', () => {
  it('describes every rule and falls back for unknown ones', () => {
    expect(Object.keys(RULES).length).toBeGreaterThan(20)
    expect(ruleInfo('http-error').label).toBe('Erreur HTTP')
    expect(ruleInfo('inconnue').label).toBe('inconnue')
  })
})
