import { readFileSync } from 'node:fs'
import { describe, expect, it } from 'vitest'
import { RULES } from './rules'

// Les textes des règles existent deux fois : `seo-rules/src/info.rs` (source de la gravité) et ici.
// Ce test lit la table Rust et exige la même liste, avec les mêmes libellés et explications.
const rust = readFileSync(new URL('../../../../crates/seo-rules/src/info.rs', import.meta.url), 'utf8')
const STR = String.raw`"((?:[^"\\]|\\.)*)"`
const unescape = (s: string) => s.replace(/\\(.)/g, '$1')

const fromRust = Object.fromEntries(
  [...rust.matchAll(new RegExp(String.raw`r\(\s*"([a-z0-9-]+)",\s*Severity::\w+,\s*${STR},\s*${STR}\s*\)`, 'g'))].map((m) => [
    m[1],
    { label: unescape(m[2]), help: unescape(m[3]) },
  ]),
)

describe('rules.ts mirrors info.rs', () => {
  it('reads the Rust table', () => {
    expect(Object.keys(fromRust).length).toBeGreaterThan(50)
  })
  it('has the same rules, labels and help texts', () => {
    expect(RULES).toEqual(fromRust)
  })
})
