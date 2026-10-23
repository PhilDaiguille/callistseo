import type { SegmentDef } from './api'

export interface SegmentForm {
  name: string
  contains: string
  prefix: string
  statusClass: string
  maxDepth: string
  issueRule: string
  extracted: string
}

export const emptySegmentForm: SegmentForm = { name: '', contains: '', prefix: '', statusClass: '', maxDepth: '', issueRule: '', extracted: '' }

const text = (v: string): string | null => (v.trim() === '' ? null : v.trim())

/** Définition envoyée à l'API : les champs vides ne sont pas des critères. */
export function toDefinition(f: SegmentForm): SegmentDef {
  const depth = Number.parseInt(f.maxDepth, 10)
  const cls = Number.parseInt(f.statusClass, 10)
  const def: SegmentDef = {
    url_contains: text(f.contains),
    url_prefix: text(f.prefix),
    status_class: Number.isFinite(cls) ? cls : null,
    max_depth: Number.isFinite(depth) && depth >= 0 ? depth : null,
    issue_rule: text(f.issueRule),
    has_extracted: text(f.extracted),
  }
  return def
}

/** Au moins un critère renseigné (l'API refuse un segment sans critère). */
export function hasCriterion(def: SegmentDef): boolean {
  return Object.values(def).some((v) => v !== null && v !== undefined)
}

/** Résumé lisible d'un segment, pour l'info-bulle de la liste. */
export function describe(def: SegmentDef): string {
  const parts: string[] = []
  if (def.url_contains) parts.push(`URL contient « ${def.url_contains} »`)
  if (def.url_prefix) parts.push(`URL commence par ${def.url_prefix}`)
  if (def.status_class) parts.push(`statut ${def.status_class}xx`)
  if (def.max_depth !== null && def.max_depth !== undefined) parts.push(`profondeur ≤ ${def.max_depth}`)
  if (def.issue_rule) parts.push(`problème ${def.issue_rule}`)
  if (def.has_extracted) parts.push(`extraction « ${def.has_extracted} » trouvée`)
  return parts.join(' · ')
}
