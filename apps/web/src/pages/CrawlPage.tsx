import { useEffect, useState } from 'react'
import { useParams } from 'react-router-dom'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { RunBadge } from '@/components/Badges'
import { PageSkeleton } from '@/components/Skeleton'
import { QueryError } from '@/components/QueryError'
import { RunProgress } from '@/components/RunProgress'
import { Button } from '@/components/ui/button'
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { api, downloadExport, downloadSitemap } from '@/lib/api'
import { EXAMPLES_PER_RULE, issuesMarkdown } from '@/lib/aiReport'
import { formatDateAt, formatDuration } from '@/lib/format'
import { useDocumentTitle } from '@/lib/title'
import { useQueryParams } from '@/lib/useQueryParams'
import { AuditOverview } from './AuditOverview'
import { DiffView } from './DiffView'
import { LinksView } from './LinksView'
import { Workstation } from './Workstation'

const TABS = ['overview', 'data', 'links', 'diff'] as const
type Tab = (typeof TABS)[number]

export function CrawlPage() {
  const id = Number(useParams().id)
  const queryClient = useQueryClient()
  const [params, update] = useQueryParams()
  const [exportError, setExportError] = useState<unknown>(null)
  const tabParam = params.get('tab')

  const crawl = useQuery({
    queryKey: ['crawl', id],
    queryFn: () => api.crawl(id),
    refetchInterval: (q) => (q.state.data?.status === 'running' ? 1000 : false),
  })
  const cancel = useMutation({
    mutationFn: () => api.cancel(id),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['crawl', id] }),
  })
  const control = useMutation({
    mutationFn: (action: 'pause' | 'resume' | 'stop') => api.control(id, action),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['crawl', id] }),
  })
  const download = (format: 'csv' | 'json' | 'html', kind: 'pages' | 'issues' = 'pages') =>
    downloadExport(id, format, kind).then(() => setExportError(null), setExportError)

  const [copied, setCopied] = useState(false)
  // Une requête par règle présente (10 exemples chacune) : le texte reste court pour une IA.
  const copyIssues = async () => {
    try {
      const first = await api.issues(id, { limit: 1, offset: 0 })
      const groups = await Promise.all(
        first.summary.map(async (g) => ({
          ...g,
          examples: (await api.issues(id, { rule: g.rule, limit: EXAMPLES_PER_RULE, offset: 0 })).items,
        })),
      )
      await navigator.clipboard.writeText(issuesMarkdown(crawl.data!, groups))
      setExportError(null)
      setCopied(true)
      setTimeout(() => setCopied(false), 2000)
    } catch (e) {
      setExportError(e)
    }
  }

  // Pendant l'audit, le tableau se remplit : on relit ses données toutes les 3 s, puis une
  // dernière fois à la fin (problèmes et maillage n'existent qu'à ce moment).
  const status = crawl.data?.status
  useEffect(() => {
    const refresh = () => {
      for (const key of ['reports', 'rows', 'segments', 'page-detail']) void queryClient.invalidateQueries({ queryKey: [key, id] })
    }
    if (status !== 'running') {
      refresh()
      return
    }
    const timer = setInterval(refresh, 3000)
    return () => clearInterval(timer)
  }, [status, id, queryClient])

  const reports = useQuery({ queryKey: ['reports', id], queryFn: () => api.reports(id) })
  const projectId = crawl.data?.project_id
  // L'historique du projet sert à situer cet audit par rapport au précédent.
  const history = useQuery({ queryKey: ['project', projectId], queryFn: () => api.project(projectId ?? 0), enabled: projectId !== undefined })
  useDocumentTitle(crawl.data ? `Audit #${id} · ${crawl.data.project}` : null)

  if (crawl.error) return <QueryError error={crawl.error} />
  if (!crawl.data) return <PageSkeleton />
  const c = crawl.data
  const running = c.status === 'running'
  // Vue d'ensemble par défaut une fois l'audit terminé ; un ancien lien `?r=<rapport>` ouvre l'explorateur.
  const report = params.get('r')
  const tab = TABS.includes(tabParam as Tab) ? (tabParam as Tab) : running || (report && report !== 'overview') ? 'data' : 'overview'
  const showTab = (t: string) => update({ tab: t, ...(report === 'overview' ? { r: null } : {}) })

  return (
    <div className="flex flex-col gap-4 lg:h-full lg:min-h-0">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="flex flex-wrap items-center gap-3">
            <h1 className="text-2xl font-extrabold tracking-tight">Audit #{c.id}</h1>
            <RunBadge status={c.status} />
            {running && (
              <div className="flex flex-wrap gap-2">
                <Button
                  variant="outline"
                  size="sm"
                  disabled={control.isPending || c.live?.stopping}
                  onClick={() => control.mutate(c.live?.paused ? 'resume' : 'pause')}
                >
                  {c.live?.paused ? 'Reprendre' : 'Pause'}
                </Button>
                <Button variant="outline" size="sm" disabled={control.isPending || c.live?.stopping} onClick={() => control.mutate('stop')}>
                  Arrêter et garder les résultats
                </Button>
                <Button variant="destructive" size="sm" disabled={cancel.isPending} onClick={() => cancel.mutate()}>
                  Annuler
                </Button>
              </div>
            )}
          </div>
          <p className="min-w-0 text-sm text-muted-foreground [overflow-wrap:anywhere]">
            <a href={c.start_url} target="_blank" rel="noopener noreferrer" className="text-foreground underline-offset-4 hover:underline" title={c.start_url}>
              {c.start_url}
            </a>
            , lancé le {formatDateAt(c.started_at)}, {running ? `depuis ${formatDuration(c.started_at, null)}` : `durée ${formatDuration(c.started_at, c.finished_at)}`}
          </p>
        </div>
        <div className="flex shrink-0 flex-wrap items-center gap-2">
          <Button variant="outline" size="sm" disabled={running} onClick={() => void copyIssues()}>
            {copied ? 'Copié ✓' : 'Copier les problèmes (IA)'}
          </Button>
          <details className="relative">
            <summary className="inline-flex h-8 cursor-pointer list-none items-center rounded-lg border border-line bg-card px-3 shadow-pop-sm text-[0.8rem] font-semibold aria-disabled:opacity-50">
              Exporter ▾
            </summary>
            <div className="panel-pop absolute right-0 z-20 mt-1 grid w-52 gap-0.5 p-1">
              {(
                [
                  ['CSV des URL', () => download('csv', 'pages')],
                  ['CSV des problèmes', () => download('csv', 'issues')],
                  ['JSON', () => download('json')],
                  ['Sitemap XML', () => downloadSitemap(id).then(() => setExportError(null), setExportError)],
                  ['Rapport HTML', () => download('html')],
                ] as [string, () => Promise<void>][]
              ).map(([label, run]) => (
                <button
                  key={label}
                  type="button"
                  disabled={running}
                  onClick={() => void run()}
                  className="rounded-lg px-3 py-1.5 text-left text-sm hover:bg-accent disabled:opacity-50"
                >
                  {label}
                </button>
              ))}
            </div>
          </details>
        </div>
      </div>

      {c.error && <QueryError error={new Error(c.error)} />}
      <QueryError error={cancel.error} />
      <QueryError error={control.error} />
      <QueryError error={exportError} />

      {running && (
        <div className="panel flex flex-wrap items-center gap-x-4 gap-y-1 px-4 py-3 text-sm" role="status" aria-live="polite">
          <span className="font-semibold">
            {c.live?.paused ? 'Audit en pause' : c.live?.stopping ? 'Arrêt en cours…' : `Audit en cours : ${c.pages.toLocaleString('fr-FR')} URL explorées`}
          </span>
          <RunProgress pages={c.pages} live={c.live} className="order-last m-0 max-w-none" />
          {c.live && (
            <span className="text-muted-foreground">
              {c.live.urls_per_second.toLocaleString('fr-FR', { maximumFractionDigits: 1 })} URL/s, {c.live.queued.toLocaleString('fr-FR')} en file, {Math.round(c.live.avg_latency_ms)} ms par page
              {c.live.failures > 0 && `, ${c.live.failures} échec(s)`}
            </span>
          )}
          <span className="basis-full text-muted-foreground">
            Les URL explorées s’affichent au fil de l’eau ; problèmes, liens entrants et maillage sont calculés à la fin.
          </span>
        </div>
      )}

      <div className="flex min-h-0 flex-1 flex-col gap-4">
        <Tabs value={tab} onValueChange={(v) => showTab(String(v))}>
          <TabsList>
            <TabsTrigger value="overview" disabled={running}>
              Vue d’ensemble
            </TabsTrigger>
            <TabsTrigger value="data">Explorer les URL</TabsTrigger>
            <TabsTrigger value="links" disabled={running}>
              Maillage
            </TabsTrigger>
            <TabsTrigger value="diff" disabled={running}>
              Comparaison
            </TabsTrigger>
          </TabsList>
        </Tabs>
        {tab === 'data' && (
          <div className="min-h-0 flex-1">
            <Workstation crawlId={id} projectId={c.project_id} onTab={showTab} />
          </div>
        )}
        {tab !== 'data' && !running && (
          <div className="min-h-0 flex-1 lg:overflow-auto lg:pr-1">
            {tab === 'overview' &&
              (reports.error ? (
                <QueryError error={reports.error} />
              ) : reports.data ? (
                <AuditOverview crawl={c} history={history.data?.crawls} reports={reports.data} onOpen={(r) => update({ tab: 'data', r, sel: null, sort: null, dir: null, cols: null })} />
              ) : (
                <PageSkeleton />
              ))}
            {tab === 'links' && <LinksView crawlId={id} crawlStatus={c.status} />}
            {tab === 'diff' && <DiffView crawlId={id} projectId={c.project_id} />}
          </div>
        )}
      </div>
    </div>
  )
}
