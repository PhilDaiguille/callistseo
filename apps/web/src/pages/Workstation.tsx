import { useEffect, useMemo, useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Columns3 } from 'lucide-react'
import { SeverityBadge } from '@/components/Badges'
import { SeverityBar } from '@/components/charts'
import { EmptyState } from '@/components/EmptyState'
import { QueryError } from '@/components/QueryError'
import { SearchInput } from '@/components/SearchInput'
import { Button } from '@/components/ui/button'
import { CommandPalette, type Command } from '@/components/workstation/CommandPalette'
import { DataTable } from '@/components/workstation/DataTable'
import { Inspector } from '@/components/workstation/Inspector'
import { SegmentBar } from '@/components/workstation/SegmentBar'
import { ReportSidebar } from '@/components/workstation/ReportSidebar'
import { api, downloadExport, downloadSitemap } from '@/lib/api'
import { ruleInfo } from '@/lib/rules'
import { severityTotals } from '@/lib/score'
import { loadColumns, pickColumns, saveColumns } from '@/lib/table'
import { useQueryParams } from '@/lib/useQueryParams'

/**
 * Poste de travail d'un audit terminé : rapports à gauche (avec nombre d'URL), tableau virtualisé au
 * centre, inspecteur à droite. Tout l'état (rapport, tri, colonnes, recherche, URL choisie) est dans
 * l'adresse : un lien reproduit exactement la vue.
 */
/** `running` : audit en cours, sans problèmes calculés ; la vue par défaut est alors la liste des URL. */
export function Workstation({
  crawlId,
  projectId,
  onTab,
  running = false,
}: {
  crawlId: number
  projectId: number
  onTab: (tab: string) => void
  running?: boolean
}) {
  const [params, update] = useQueryParams()
  const [paletteOpen, setPaletteOpen] = useState(false)
  const reports = useQuery({ queryKey: ['reports', crawlId], queryFn: () => api.reports(crawlId) })

  const report = params.get('r') ?? (running ? 'all' : 'overview')
  const sort = params.get('sort')
  const dir = params.get('dir') === 'desc' ? 'desc' : 'asc'
  const search = params.get('q') ?? ''
  const selected = params.get('sel')
  const segmentParam = Number.parseInt(params.get('seg') ?? '', 10)
  const segment = Number.isFinite(segmentParam) ? segmentParam : null

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k') {
        e.preventDefault()
        setPaletteOpen((o) => !o)
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [])

  const known = useMemo(() => reports.data?.columns.map((c) => c.id) ?? [], [reports.data])
  const current = useMemo(() => {
    const data = reports.data
    if (!data) return null
    const entry = data.groups.flatMap((g) => g.reports).find((r) => r.id === report)
    const issue = data.issues.find((i) => i.id === report)
    return { label: entry?.label ?? issue?.label ?? '', defaults: entry?.columns ?? ['url', 'status', 'title', 'issues'], count: entry?.count ?? issue?.count ?? 0 }
  }, [reports.data, report])

  const commands: Command[] = useMemo(() => {
    const data = reports.data
    if (!data) return []
    const go = (id: string) => () => update({ r: id, sort: null, dir: null, cols: null, sel: null })
    return [
      { id: 'overview', label: 'À corriger en premier', hint: 'Vue d’ensemble', run: go('overview') },
      ...data.issues.map((i) => ({ id: i.id, label: i.label, hint: `Problème · ${i.count}`, run: go(i.id) })),
      ...data.groups.flatMap((g) => g.reports.map((r) => ({ id: r.id, label: r.label, hint: `${g.label} · ${r.count}`, run: go(r.id) }))),
      { id: 'tab-links', label: 'Maillage interne', hint: 'Vue', run: () => onTab('links') },
      { id: 'tab-diff', label: 'Comparer avec un autre audit', hint: 'Vue', run: () => onTab('diff') },
      { id: 'x-csv', label: 'Exporter les URL (CSV)', hint: 'Export', run: () => void downloadExport(crawlId, 'csv', 'pages') },
      { id: 'x-issues', label: 'Exporter les problèmes (CSV)', hint: 'Export', run: () => void downloadExport(crawlId, 'csv', 'issues') },
      { id: 'x-json', label: 'Exporter en JSON', hint: 'Export', run: () => void downloadExport(crawlId, 'json') },
      { id: 'x-html', label: 'Rapport HTML', hint: 'Export', run: () => void downloadExport(crawlId, 'html') },
      { id: 'x-sitemap', label: 'Générer le sitemap XML', hint: 'Export', run: () => void downloadSitemap(crawlId) },
    ]
  }, [reports.data, crawlId, onTab, update])

  if (reports.error) return <QueryError error={reports.error} />
  if (!reports.data || !current) return <p className="text-sm text-muted-foreground">Chargement…</p>
  const data = reports.data
  // Colonnes : l'adresse d'abord (lien partagé), puis le dernier choix mémorisé pour ce rapport, puis le défaut.
  const columns = pickColumns(params.get('cols') ?? loadColumns(report), current.defaults, known)
  const toggleColumn = (id: string) => {
    const next = columns.includes(id) ? columns.filter((c) => c !== id) : [...columns, id]
    if (next.length > 0) {
      saveColumns(report, next)
      update({ cols: next.join(',') })
    }
  }

  return (
    <div
      className={`grid gap-3 max-lg:h-[75svh] lg:h-full lg:min-h-[24rem] lg:grid-cols-[15rem_minmax(0,1fr)] ${selected ? 'xl:grid-cols-[15rem_minmax(0,1fr)_22rem]' : ''}`}
    >
      <div className="lg:overflow-auto lg:pr-1">
        <details className="lg:hidden" open={false}>
          <summary className="panel cursor-pointer px-3 py-2 text-sm font-semibold">Rapports : {report === 'overview' ? 'À corriger en premier' : current.label}</summary>
          <div className="panel mt-2 p-2">
            <ReportSidebar reports={data} active={report} onSelect={(id) => update({ r: id, sort: null, dir: null, cols: null, sel: null })} />
          </div>
        </details>
        <div className="hidden lg:block">
          <ReportSidebar reports={data} active={report} onSelect={(id) => update({ r: id, sort: null, dir: null, cols: null, sel: null })} />
        </div>
      </div>

      <div className="flex min-h-0 min-w-0 flex-col gap-2">
        {report === 'overview' ? (
          <Overview data={data} onOpen={(id) => update({ r: id, sel: null })} />
        ) : (
          <>
            <h2 className="text-lg font-bold">{current.label}</h2>
            <SegmentBar crawlId={crawlId} projectId={projectId} active={segment} issues={data.issues} onSelect={(id) => update({ seg: id === null ? null : String(id), sel: null })} />
            <div className="flex flex-wrap items-center gap-2">
              <div className="min-w-56 flex-1">
                <SearchInput className="max-w-md" value={search} onChange={(v) => update({ q: v || null, sel: null })} placeholder="Filtrer par URL ou title…" />
              </div>
              <details className="relative">
                <summary className="inline-flex h-8 cursor-pointer list-none items-center gap-1.5 rounded-lg border border-line bg-card px-3 shadow-pop-sm text-[0.8rem] font-semibold">
                  <Columns3 className="size-3.5" aria-hidden /> Colonnes
                </summary>
                <div className="panel-pop absolute right-0 z-20 mt-1 max-h-80 w-56 overflow-auto p-2">
                  {data.columns.map((c) => (
                    <label key={c.id} className="flex cursor-pointer items-center gap-2 rounded px-1 py-0.5 text-sm hover:bg-accent">
                      <input type="checkbox" checked={columns.includes(c.id)} onChange={() => toggleColumn(c.id)} />
                      {c.label}
                    </label>
                  ))}
                </div>
              </details>
              <Button variant="outline" size="sm" onClick={() => setPaletteOpen(true)} aria-keyshortcuts="Control+K Meta+K">
                ⌘K
              </Button>
            </div>
            <DataTable
              crawlId={crawlId}
              state={{ report, columns, sort, dir, search, segment }}
              defs={data.columns}
              selected={selected}
              onSelect={(url) => update({ sel: url })}
              onSort={(c) => update(sort === c ? (dir === 'asc' ? { dir: 'desc' } : { sort: null, dir: null }) : { sort: c, dir: 'asc' })}
            />
          </>
        )}
      </div>

      {selected && (
        <div className="fixed inset-0 z-30 bg-background p-3 xl:static xl:z-auto xl:min-h-0 xl:bg-transparent xl:p-0">
          <Inspector crawlId={crawlId} url={selected} onClose={() => update({ sel: null })} />
        </div>
      )}
      <CommandPalette commands={commands} open={paletteOpen} onOpenChange={setPaletteOpen} />
    </div>
  )
}

/** Accueil : répartition par gravité, puis les problèmes triés par gravité et nombre d'URL. */
function Overview({ data, onOpen }: { data: import('@/lib/api').ReportsResponse; onOpen: (id: string) => void }) {
  if (data.issues.length === 0) {
    return <EmptyState mood="happy">Aucun problème détecté sur cet audit. Bravo !</EmptyState>
  }
  return (
    <div className="grid content-start gap-3 overflow-auto lg:pr-1">
      <section className="panel grid gap-2 px-4 py-3" aria-labelledby="repartition">
        <h2 id="repartition" className="text-sm font-semibold">
          URL concernées par gravité
        </h2>
        <SeverityBar totals={severityTotals(data.issues)} />
      </section>
      <section className="grid gap-2" aria-labelledby="a-corriger">
        <div className="flex flex-wrap items-baseline justify-between gap-2">
          <h2 id="a-corriger" className="text-lg font-bold">
            À corriger en premier
          </h2>
          <p className="text-xs text-muted-foreground">Triés par gravité, puis par nombre d’URL concernées.</p>
        </div>
        <ul className="table-panel divide-y divide-line">
          {data.issues.map((i) => (
            <li key={i.id}>
              <button type="button" onClick={() => onOpen(i.id)} className="grid w-full gap-0.5 px-4 py-2.5 text-left transition-colors hover:bg-accent">
                <div className="flex flex-wrap items-center gap-2">
                  <SeverityBadge severity={i.severity} />
                  <span className="font-semibold">{i.label}</span>
                  <span className="ml-auto text-sm font-semibold tabular-nums">{i.count.toLocaleString('fr-FR')} URL</span>
                </div>
                <p className="text-xs text-muted-foreground">{ruleInfo(i.rule).help}</p>
              </button>
            </li>
          ))}
        </ul>
      </section>
    </div>
  )
}
