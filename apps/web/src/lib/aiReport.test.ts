import { describe, expect, it } from 'vitest'
import type { Crawl } from './api'
import { issuesMarkdown } from './aiReport'

const crawl = { id: 7, start_url: 'https://a.test/', pages: 12, errors: 1, warnings: 2 } as Crawl

describe('issuesMarkdown', () => {
  it('orders by severity, lists examples and counts the rest', () => {
    const md = issuesMarkdown(crawl, [
      { rule: 'h1-missing', severity: 'Warning', count: 2, examples: [{ severity: 'Warning', rule: 'h1-missing', url: 'https://a.test/x', message: '' }] },
      { rule: 'broken-link', severity: 'Error', count: 1, examples: [{ severity: 'Error', rule: 'broken-link', url: 'https://a.test/', message: 'vers /404' }] },
    ])
    expect(md.indexOf('broken-link')).toBeLessThan(md.indexOf('h1-missing'))
    expect(md).toContain('- https://a.test/ — vers /404')
    expect(md).toContain('- … et 1 autre(s)')
    expect(md).toContain('12 URL explorées, 1 erreurs, 2 avertissements')
  })
})
