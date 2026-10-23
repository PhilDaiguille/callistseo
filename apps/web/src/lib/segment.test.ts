import { describe, expect, it } from 'vitest'
import { describe as describeSegment, emptySegmentForm, hasCriterion, toDefinition } from './segment'

describe('segment form', () => {
  it('ignores blank fields', () => {
    const def = toDefinition({ ...emptySegmentForm, name: 'x', contains: '  blog ', statusClass: '4', maxDepth: '0' })
    expect(def).toEqual({ url_contains: 'blog', url_prefix: null, status_class: 4, max_depth: 0, issue_rule: null, has_extracted: null })
    expect(hasCriterion(def)).toBe(true)
    expect(hasCriterion(toDefinition(emptySegmentForm))).toBe(false)
  })
  it('rejects negative or invalid depth silently', () => {
    expect(toDefinition({ ...emptySegmentForm, maxDepth: '-2' }).max_depth).toBeNull()
    expect(toDefinition({ ...emptySegmentForm, maxDepth: 'abc' }).max_depth).toBeNull()
  })
  it('describes a segment', () => {
    expect(describeSegment({ url_contains: 'blog', status_class: 2, max_depth: 1 })).toBe('URL contient « blog » · statut 2xx · profondeur ≤ 1')
  })
})
