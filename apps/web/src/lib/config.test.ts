import { describe, expect, it } from 'vitest'
import type { ProjectConfig } from './api'
import { formatExtractors, parseExtractors, toConfig, toForm, toLines, toNumber } from './config'

const base: ProjectConfig = {
  include: [],
  exclude: [],
  strip_params: [],
  max_url_length: null,
  robots_txt: null,
  max_pages: null,
  max_depth: null,
  delay_ms: null,
  concurrency: 4,
  extractors: [],
}

describe('config form', () => {
  it('splits patterns by line and ignores blanks', () => {
    expect(toLines(' /blog/ \n\n  \\.pdf$\n')).toEqual(['/blog/', '\\.pdf$'])
  })
  it('parses numbers, empty means default', () => {
    expect(toNumber('50')).toBe(50)
    expect(toNumber('')).toBeNull()
    expect(toNumber('abc')).toBeNull()
    expect(toNumber('-3')).toBeNull()
  })
  it('round-trips and keeps fields the form does not edit', () => {
    const cfg: ProjectConfig = { ...base, include: ['/a/'], exclude: ['x', 'y'], max_pages: 20, delay_ms: 0, robots_txt: 'User-agent: *' }
    expect(toConfig(toForm(cfg), cfg)).toEqual(cfg)
    expect(toConfig(toForm(base), base).concurrency).toBe(4)
  })
  it('turns an all-blank robots field into null', () => {
    expect(toConfig({ ...toForm(base), robots: '  \n ' }, base).robots_txt).toBeNull()
  })

  it('parses extractors, keeping colons inside the pattern', () => {
    const list = parseExtractors('ga: regex: UA-(\\d+)\nprix: css: .price\nlien: css@href: a.cta\nurl: regex: https?://x')
    expect(list.map((e) => [e.name, e.kind, e.attr])).toEqual([['ga', 'regex', null], ['prix', 'css', null], ['lien', 'css', 'href'], ['url', 'regex', null]])
    expect(list[3]!.pattern).toBe('https?://x')
    expect(parseExtractors(formatExtractors(list))).toEqual(list)
    expect(() => parseExtractors('a: xpath: //x')).toThrow(/regex, css ou contains/)
    expect(parseExtractors('')).toEqual([])
  })
})
