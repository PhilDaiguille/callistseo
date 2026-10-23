import { describe, expect, it } from 'vitest'
import { nextPreference, pickPreference, resolveTheme } from './theme'

describe('theme', () => {
  it('resolves "system" from the OS preference and honors explicit choices', () => {
    expect(resolveTheme('system', true)).toBe('dark')
    expect(resolveTheme('system', false)).toBe('light')
    expect(resolveTheme('light', true)).toBe('light')
    expect(resolveTheme('dark', false)).toBe('dark')
  })

  it('prefers the URL parameter, then the stored value, then system; ignores junk', () => {
    expect(pickPreference('?theme=dark', 'light')).toBe('dark')
    expect(pickPreference('', 'light')).toBe('light')
    expect(pickPreference('', null)).toBe('system')
    expect(pickPreference('?theme=pink', 'dark')).toBe('dark')
    expect(pickPreference('?theme=pink', 'nope')).toBe('system')
  })

  it('cycles light -> dark -> system -> light', () => {
    expect(nextPreference('light')).toBe('dark')
    expect(nextPreference('dark')).toBe('system')
    expect(nextPreference('system')).toBe('light')
  })
})
