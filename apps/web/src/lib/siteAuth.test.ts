import { describe, expect, it } from 'vitest'
import { parseHeaders } from './siteAuth'

describe('parseHeaders', () => {
  it('lit une ligne par en-tête et garde les « : » de la valeur', () => {
    expect(parseHeaders('X-Cle: abc\n\n  Authorization: Bearer a:b  ')).toEqual({
      headers: [
        { name: 'X-Cle', value: 'abc' },
        { name: 'Authorization', value: 'Bearer a:b' },
      ],
      error: null,
    })
  })
  it('signale la première ligne invalide', () => {
    expect(parseHeaders('X: 1\nsans deux-points').error).toBe('Ligne 2 : « Nom: valeur » attendu')
    expect(parseHeaders(': vide').error).toContain('Ligne 1')
  })
})
