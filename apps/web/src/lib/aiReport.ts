import type { Crawl, IssueItem, Severity } from './api'
import { ruleInfo, SEVERITY_LABEL } from './rules'

/** Exemples d'URL gardés par règle : le texte reste assez court pour une conversation avec une IA. */
export const EXAMPLES_PER_RULE = 10

export interface RuleGroup {
  rule: string
  severity: Severity
  count: number
  examples: IssueItem[]
}

const ORDER: Record<Severity, number> = { Error: 0, Warning: 1, Info: 2 }

/** Texte Markdown des problèmes d'un audit, à coller dans un agent IA. */
export function issuesMarkdown(crawl: Crawl, groups: RuleGroup[]): string {
  const sorted = [...groups].sort((a, b) => ORDER[a.severity] - ORDER[b.severity] || b.count - a.count)
  const lines = [
    `# Audit SEO de ${crawl.start_url}`,
    '',
    `${crawl.pages} URL explorées, ${crawl.errors} erreurs, ${crawl.warnings} avertissements (audit CallistSEO n°${crawl.id}).`,
    'Ce sont des constats techniques automatiques : vérifie chacun avant de le corriger. Propose pour chaque problème la correction concrète, par ordre de gravité.',
  ]
  for (const g of sorted) {
    const info = ruleInfo(g.rule)
    lines.push('', `## [${SEVERITY_LABEL[g.severity]}] ${info.label} (\`${g.rule}\`) : ${g.count} URL`, info.help)
    for (const i of g.examples) lines.push(`- ${i.url}${i.message ? ` — ${i.message}` : ''}`)
    if (g.count > g.examples.length) lines.push(`- … et ${g.count - g.examples.length} autre(s)`)
  }
  return lines.join('\n') + '\n'
}
