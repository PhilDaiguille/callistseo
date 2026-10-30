import { ChevronRight } from 'lucide-react'
import { EmptyState } from '@/components/EmptyState'
import { HealthGauge, SeverityBar } from '@/components/charts'
import { Stat } from '@/components/Stat'
import type { Crawl, IssueReport, ReportsResponse, Severity } from '@/lib/api'
import { errorDelta, healthDelta, previousDone } from '@/lib/delta'
import { ruleInfo } from '@/lib/rules'
import { SCORE_LABEL, SEVERITY_COLOR, scoreLevel, severityTotals, share } from '@/lib/score'
import { cn } from '@/lib/utils'

// Sens des gravités (même définition que `seo-rules/src/info.rs`) : on dit quoi faire, pas seulement combien.
const GROUPS: { severity: Severity; title: string; meaning: string }[] = [
  { severity: 'Error', title: 'Erreurs', meaning: 'Ce qui est cassé pour les visiteurs ou les robots : à corriger d’abord.' },
  { severity: 'Warning', title: 'Avertissements', meaning: 'Ce qui freine le référencement : à corriger ensuite.' },
  { severity: 'Info', title: 'Infos', meaning: 'À vérifier selon votre site : ce n’est pas toujours un problème.' },
]

/**
 * Vue d'ensemble d'un audit terminé : indicateurs, répartition par gravité, puis les problèmes
 * groupés par gravité (Infos repliées). Un problème ouvre l'explorateur filtré sur lui.
 */
export function AuditOverview({
  crawl,
  history,
  reports,
  onOpen,
}: {
  crawl: Crawl
  history: Crawl[] | undefined
  reports: ReportsResponse
  onOpen: (report: string) => void
}) {
  const previous = history ? previousDone(crawl, history) : null
  const delta = previous ? errorDelta(crawl, previous) : null
  const scoreDelta = healthDelta(crawl, previous)

  return (
    // Largeur bornée : sur un écran large, des lignes de 1 900 px ne se lisent pas.
    <div className="grid max-w-6xl content-start gap-6">
      <div className="grid grid-cols-2 gap-3 xl:grid-cols-4" role="group" aria-label="Indicateurs de l’audit">
        {crawl.health !== null && (
          <div className="panel-pop col-span-2 flex items-center gap-4 px-4 py-3 xl:col-span-1">
            <HealthGauge score={crawl.health} size={72} />
            <div className="min-w-0">
              <p className="font-heading text-lg font-extrabold">{SCORE_LABEL[scoreLevel(crawl.health)]}</p>
              <p className="text-xs text-muted-foreground">Part des URL sans erreur</p>
              {scoreDelta !== null && previous && (
                <p className={cn('mt-0.5 text-xs', scoreDelta < 0 ? 'font-semibold text-destructive' : scoreDelta > 0 ? 'font-semibold' : 'text-muted-foreground')}>
                  {scoreDelta > 0 ? '▲ +' : scoreDelta < 0 ? '▼ −' : '= '}
                  {Math.abs(scoreDelta)} pts depuis l’audit #{previous.id}
                </p>
              )}
            </div>
          </div>
        )}
        <Stat label="URL explorées" value={crawl.pages} />
        <Stat
          label="Erreurs"
          value={crawl.errors}
          hint={
            delta && (
              <span className={delta.tone === 'worse' ? 'font-semibold text-destructive' : delta.tone === 'better' ? 'font-semibold' : 'text-muted-foreground'}>
                {delta.tone === 'worse' ? '▲' : delta.tone === 'better' ? '▼' : '='} {delta.text}
              </span>
            )
          }
        />
        <Stat label="Avertissements" value={crawl.warnings} />
      </div>

      {reports.issues.length === 0 ? (
        <EmptyState mood="happy">Aucun problème détecté sur cet audit.</EmptyState>
      ) : (
        <>
          <section className="panel grid gap-2 px-4 py-3" aria-labelledby="gravites">
            <div className="flex flex-wrap items-baseline justify-between gap-x-4">
              <h2 id="gravites" className="text-base font-bold">
                Problèmes par gravité
              </h2>
              <p className="text-xs text-muted-foreground">Une URL peut cumuler plusieurs problèmes.</p>
            </div>
            <SeverityBar totals={severityTotals(reports.issues)} />
          </section>

          {GROUPS.map((g) => {
            const issues = reports.issues.filter((i) => i.severity === g.severity)
            if (issues.length === 0) return null
            const list = <IssueList issues={issues} pages={crawl.pages} onOpen={onOpen} />
            const heading = (
              <>
                <h2 className="text-lg font-bold">
                  {g.title} <span className="font-sans text-sm font-semibold text-muted-foreground tabular-nums">({issues.length})</span>
                </h2>
                <p className="text-sm text-muted-foreground">{g.meaning}</p>
              </>
            )
            // Les infos sont repliées : elles ne doivent pas noyer ce qui est à corriger.
            return g.severity === 'Info' ? (
              <details key={g.severity} className="group grid gap-2">
                <summary className="flex cursor-pointer list-none items-start gap-2 rounded-lg">
                  <ChevronRight className="mt-1.5 size-4 shrink-0 transition-transform group-open:rotate-90 motion-reduce:transition-none" aria-hidden />
                  <div>{heading}</div>
                </summary>
                <div className="mt-2">{list}</div>
              </details>
            ) : (
              <section key={g.severity} className="grid gap-2">
                <div>{heading}</div>
                {list}
              </section>
            )
          })}
        </>
      )}
    </div>
  )
}

function IssueList({ issues, pages, onOpen }: { issues: IssueReport[]; pages: number; onOpen: (report: string) => void }) {
  return (
    <ul className="table-panel divide-y divide-line">
      {issues.map((i) => {
        const { ratio, text } = share(i.count, pages)
        const help = ruleInfo(i.rule).help
        return (
          <li key={i.id}>
            <button
              type="button"
              onClick={() => onOpen(i.id)}
              className="grid w-full gap-x-4 gap-y-1 px-4 py-2.5 text-left transition-colors hover:bg-accent sm:grid-cols-[minmax(0,1fr)_11rem] sm:items-center"
            >
              <span className="min-w-0">
                <span className="block font-semibold">{i.label}</span>
                <span className="block truncate text-xs text-muted-foreground" title={help}>
                  {help}
                </span>
              </span>
              <span className="grid gap-1">
                <span className="flex items-baseline justify-between gap-2 text-sm tabular-nums">
                  <span className="font-semibold">{i.count.toLocaleString('fr-FR')} URL</span>
                  <span className="text-xs text-muted-foreground">{text}</span>
                </span>
                <span aria-hidden className="h-1.5 overflow-hidden rounded-full bg-muted">
                  <span className="block h-full rounded-full" style={{ width: `${Math.max(ratio * 100, 2)}%`, background: SEVERITY_COLOR[i.severity] }} />
                </span>
              </span>
            </button>
          </li>
        )
      })}
    </ul>
  )
}
