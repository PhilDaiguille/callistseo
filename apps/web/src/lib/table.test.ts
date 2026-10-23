import { describe, expect, it } from 'vitest'
import { columnTemplate, fold, formatBytes, formatCell, loadColumns, matches, pickColumns, saveColumns, widthOf, wrap } from './table'

describe('formatCell', () => {
  it('formats by column', () => {
    expect(formatCell('title', null)).toBe('—')
    expect(formatCell('title', '')).toBe('—')
    expect(formatCell('size', 2048)).toBe('2,0 Ko')
    expect(formatCell('ttfb_ms', 120)).toBe('120 ms')
    expect(formatCell('status', 404)).toBe('404')
    expect(formatCell('in_sitemap', 1)).toBe('Oui')
    expect(formatCell('in_sitemap', 0)).toBe('Non')
    expect(formatCell('url', 'http://a.test/')).toBe('http://a.test/')
  })
  it('formats bytes', () => {
    expect(formatBytes(12)).toBe('12 o')
    expect(formatBytes(1536)).toBe('1,5 Ko')
    expect(formatBytes(5 * 1024 * 1024)).toBe('5,0 Mo')
  })
})

describe('search helpers', () => {
  it('ignores accents and case, all words must match', () => {
    expect(fold('Écran Été')).toBe('ecran ete')
    expect(matches('ecran ete', 'Écran de l’Été')).toBe(true)
    expect(matches('ete ecran', 'Écran de l’Été')).toBe(true)
    expect(matches('ecran hiver', 'Écran de l’Été')).toBe(false)
    expect(matches('', 'tout')).toBe(true)
  })
})

describe('wrap', () => {
  it('wraps both ways and survives empty lists', () => {
    expect(wrap(3, 2, 1)).toBe(0)
    expect(wrap(3, 0, -1)).toBe(2)
    expect(wrap(0, 0, 1)).toBe(-1)
  })
})

describe('pickColumns', () => {
  it('keeps known columns in asked order, else falls back', () => {
    expect(pickColumns('title,url,nope', ['url'], ['url', 'title'])).toEqual(['title', 'url'])
    expect(pickColumns(null, ['url', 'status'], ['url', 'status'])).toEqual(['url', 'status'])
    expect(pickColumns('nope', ['url'], ['url'])).toEqual(['url'])
  })
  it('has a default width', () => {
    expect(widthOf('url')).toBe(420)
    expect(widthOf('depth')).toBe(120)
  })
})

describe('remembered columns', () => {
  it('never throws without storage (node, private mode)', () => {
    expect(loadColumns('all')).toBeNull()
    expect(() => saveColumns('all', ['url'])).not.toThrow()
  })
})

describe('columnTemplate', () => {
  it("laisse l'URL absorber l'espace libre", () => {
    expect(columnTemplate(['status', 'url', 'title'])).toBe(`${widthOf('status')}px minmax(${widthOf('url')}px, 1fr) ${widthOf('title')}px`)
  })

  it('sans colonne url, la dernière colonne absorbe le reste', () => {
    expect(columnTemplate(['status', 'title'])).toBe(`${widthOf('status')}px minmax(${widthOf('title')}px, 1fr)`)
  })
})
