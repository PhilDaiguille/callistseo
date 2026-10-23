import { readFileSync } from 'node:fs'
import { describe, expect, it } from 'vitest'

// Lit les couleurs hexadécimales de index.css et vérifie le contraste WCAG AA (4,5:1) des paires
// texte/fond réellement utilisées. Échoue si quelqu'un casse la palette.
const css = readFileSync(new URL('../index.css', import.meta.url), 'utf8')

function tokens(selector: string): Record<string, string> {
  const block = css.match(new RegExp(`${selector.replace('.', '\\.')}\\s*\\{([^}]*)\\}`))?.[1] ?? ''
  return Object.fromEntries([...block.matchAll(/--([a-z-]+):\s*(#[0-9a-fA-F]{6})/g)].map((m) => [m[1], m[2]]))
}

function luminance(hex: string): number {
  const channel = (i: number) => {
    const c = parseInt(hex.slice(1 + i * 2, 3 + i * 2), 16) / 255
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4
  }
  return 0.2126 * channel(0) + 0.7152 * channel(1) + 0.0722 * channel(2)
}

export function contrast(a: string, b: string): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x)
  return (hi + 0.05) / (lo + 0.05)
}

// [texte, fond] : ce que les composants affichent réellement.
const PAIRS: [string, string][] = [
  ['foreground', 'background'],
  ['foreground', 'card'],
  ['muted-foreground', 'background'],
  ['muted-foreground', 'card'],
  ['primary-foreground', 'primary'], // bouton principal, onglet actif
  ['secondary-foreground', 'secondary'],
  ['accent-foreground', 'accent'], // survol, sélection
  ['foreground', 'success-soft'], // badges
  ['foreground', 'warning-soft'],
  ['foreground', 'info-soft'],
  ['destructive', 'destructive-soft'], // badge et bouton destructifs
  ['destructive', 'background'], // message d'erreur
]

describe('palette contrast (WCAG AA)', () => {
  it('computes known ratios correctly', () => {
    expect(contrast('#000000', '#ffffff')).toBeCloseTo(21, 0)
    expect(contrast('#777777', '#ffffff')).toBeCloseTo(4.48, 1)
  })

  for (const [name, selector] of [
    ['light', ':root'],
    ['dark', '.dark'],
  ] as const) {
    it(`${name} theme: every text/background pair reaches 4.5:1`, () => {
      const t = tokens(selector)
      const low: string[] = []
      for (const [fg, bg] of PAIRS) {
        expect(t[fg], `${fg} manquant dans ${selector}`).toBeTruthy()
        expect(t[bg], `${bg} manquant dans ${selector}`).toBeTruthy()
        const ratio = contrast(t[fg], t[bg])
        if (ratio < 4.5) low.push(`${fg} sur ${bg} : ${ratio.toFixed(2)}`)
      }
      expect(low, `contrastes insuffisants (${name})`).toEqual([])
    })
  }
})
