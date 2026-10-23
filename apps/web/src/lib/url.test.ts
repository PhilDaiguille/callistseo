import { describe, expect, it } from 'vitest'
import { detailPath, isBrokenLink, isWebUrl, shortUrl } from './url'

describe('url', () => {
  it('encode l\'URL dans le chemin de la fiche', () => {
    expect(detailPath(3, 'https://a.test/x?q=1&r=2')).toBe('/crawls/3/url?u=https%3A%2F%2Fa.test%2Fx%3Fq%3D1%26r%3D2')
  })

  it('n\'accepte que http et https', () => {
    expect(isWebUrl('https://a.test/')).toBe(true)
    expect(isWebUrl('http://a.test/x')).toBe(true)
    expect(isWebUrl('javascript:alert(1)')).toBe(false)
    expect(isWebUrl('file:///etc/passwd')).toBe(false)
    expect(isWebUrl('pas une url')).toBe(false)
  })

  it('abrège une URL en chemin et paramètres', () => {
    expect(shortUrl('https://a.test/p/12?x=1#f')).toBe('/p/12?x=1')
    expect(shortUrl('https://a.test')).toBe('/')
    expect(shortUrl('pas une url')).toBe('pas une url')
  })

  it('repère les liens cassés, pas les pages non explorées', () => {
    const l = (status: number | null, crawled: boolean) => ({ url: 'u', status, crawled })
    expect(isBrokenLink(l(404, true))).toBe(true)
    expect(isBrokenLink(l(null, true))).toBe(true)
    expect(isBrokenLink(l(200, true))).toBe(false)
    expect(isBrokenLink(l(301, true))).toBe(false)
    expect(isBrokenLink(l(null, false))).toBe(false)
  })
})
