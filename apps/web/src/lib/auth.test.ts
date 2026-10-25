import { describe, expect, it } from 'vitest'
import { loginPath, safeNext } from './api'

describe('connexion', () => {
  it('renvoie vers /login en gardant la page demandée', () => {
    expect(loginPath('/crawls/3', '?r=all')).toBe('/login?next=%2Fcrawls%2F3%3Fr%3Dall')
    expect(loginPath('/login', '?next=%2F')).toBeNull()
  })

  it("n'accepte qu'un retour vers un chemin interne", () => {
    expect(safeNext('/projects/2')).toBe('/projects/2')
    expect(safeNext(null)).toBe('/')
    expect(safeNext('https://evil.example')).toBe('/')
    expect(safeNext('//evil.example')).toBe('/')
    expect(safeNext('/\\evil.example')).toBe('/')
  })
})
