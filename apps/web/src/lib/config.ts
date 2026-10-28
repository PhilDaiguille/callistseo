import type { ExtractorDef, ProjectConfig } from './api'

/** Un motif par ligne : lignes vides et espaces de bord ignorés. */
export const toLines = (text: string): string[] =>
  text
    .split('\n')
    .map((l) => l.trim())
    .filter(Boolean)

export const fromLines = (items: string[]): string => items.join('\n')

/** Nombre entier positif saisi dans un champ ; vide ou invalide = `null` (« valeur par défaut »). */
export function toNumber(text: string): number | null {
  const n = Number.parseInt(text, 10)
  return Number.isFinite(n) && n > 0 ? n : null
}

/** Une ligne par extracteur : `nom: type: motif` ; `css@href` lit l'attribut `href` au lieu du texte. */
export function parseExtractors(text: string): ExtractorDef[] {
  return toLines(text).map((line) => {
    const [name = '', kindRaw = '', ...rest] = line.split(':')
    const [kind = '', attr] = kindRaw.trim().split('@')
    if (!['regex', 'css', 'contains', 'xpath'].includes(kind)) {
      throw new Error(`Extraction « ${line.slice(0, 40)} » : le type doit être regex, css, contains ou xpath (format : nom: type: motif)`)
    }
    return { name: name.trim(), kind: kind as ExtractorDef['kind'], pattern: rest.join(':').trim(), attr: attr ? attr.trim() : null }
  })
}

export function formatExtractors(list: ExtractorDef[]): string {
  return list.map((e) => `${e.name}: ${e.kind}${e.attr ? `@${e.attr}` : ''}: ${e.pattern}`).join('\n')
}

export interface ConfigForm {
  extractors: string
  include: string
  exclude: string
  strip: string
  maxUrlLength: string
  robots: string
  maxPages: string
  maxDepth: string
  delayMs: string
}

const text = (n: number | null) => (n === null ? '' : String(n))

export function toForm(c: ProjectConfig): ConfigForm {
  return {
    extractors: formatExtractors(c.extractors),
    include: fromLines(c.include),
    exclude: fromLines(c.exclude),
    strip: fromLines(c.strip_params),
    maxUrlLength: text(c.max_url_length),
    robots: c.robots_txt ?? '',
    maxPages: text(c.max_pages),
    maxDepth: text(c.max_depth),
    delayMs: text(c.delay_ms),
  }
}

export function toConfig(f: ConfigForm, previous: ProjectConfig): ProjectConfig {
  return {
    ...previous,
    extractors: parseExtractors(f.extractors),
    include: toLines(f.include),
    exclude: toLines(f.exclude),
    strip_params: toLines(f.strip),
    max_url_length: toNumber(f.maxUrlLength),
    robots_txt: f.robots.trim() === '' ? null : f.robots,
    max_pages: toNumber(f.maxPages),
    max_depth: toNumber(f.maxDepth),
    delay_ms: f.delayMs.trim() === '0' ? 0 : toNumber(f.delayMs),
  }
}
