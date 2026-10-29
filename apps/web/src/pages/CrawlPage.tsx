import { useEffect, useState } from 'react'
import { useParams } from 'react-router-dom'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { RunBadge } from '@/components/Badges'
import { PageSkeleton } from '@/components/Skeleton'
import { QueryError } from '@/components/QueryError'
import { RunProgress } from '@/components/RunProgress'
import { HealthGauge } from '@/components/charts'
import { Stat } from '@/components/Stat'
import { Button } from '@/components/ui/button'
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { api, downloadExport, downloadSitemap } from '@/lib/api'
import { EXAMPLES_PER_RULE, issuesMarkdown } from '@/lib/aiReport'
import { errorDelta, healthDelta, previousDone } from '@/lib/delta'
import { SCORE_LABEL, scoreLevel } from '@/lib/score'
import { cn } from '@/lib/utils'
import { formatDate, formatDuration } from '@/lib/format'
import { useDocumentTitle } from '@/lib/title'
import { useQueryParams } from '@/lib/useQueryParams'
import { DiffView } from './DiffView'
import { LinksView } from './LinksView'
import { Workstation } from './Workstation'

export function CrawlPage() {
  const id = Number(useParams().id)
  const queryClient = useQueryClient()
  const [params, update] = useQueryParams()
  const [exportError, setExportError] = useState<unknown>(null)
  const tabParam = params.get('tab')
  const tab = tabParam === 'links' || tabParam === 'diff' ? tabParam : 'data'

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

  const projectId = crawl.data?.project_id
  // L'historique du projet sert à situer cet audit par rapport au précédent.
  const history = useQuery({ queryKey: ['project', projectId], queryFn: () => api.project(projectId ?? 0), enabled: projectId !== undefined })
  useDocumentTitle(crawl.data ? `Audit #${id} · ${crawl.data.project}` : null)

  if (crawl.error) return <QueryError error={crawl.error} />
  if (!crawl.data) return <PageSkeleton />
  const c = crawl.data
  const previous = c.status === 'done' && history.data ? previousDone(c, history.data.crawls) : null
  const delta = previous ? errorDelta(c, previous) : null
  const scoreDelta = healthDelta(c, previous)
  const running = c.status === 'running'

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
          <p className="min-w-0 truncate text-sm text-muted-foreground" title={c.start_url}>
            {c.start_url} · {formatDate(c.started_at)} · {formatDuration(c.started_at, c.finished_at)}
            {running && ' (en cours)'}
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

      <div className="grid shrink-0 grid-cols-2 gap-3 xl:grid-cols-4" aria-label="Résumé de l’audit" role="group">
        {c.health !== null && (
          <div className="panel-pop col-span-2 flex items-center gap-4 px-4 py-3 xl:col-span-1">
            <HealthGauge score={c.health} size={72} />
            <div className="min-w-0">
              <p className="text-xs font-medium text-muted-foreground">Score de santé</p>
              <p className="font-heading text-lg font-extrabold">{SCORE_LABEL[scoreLevel(c.health)]}</p>
              {scoreDelta !== null && previous && (
                <p className={cn('text-xs', scoreDelta < 0 ? 'font-semibold text-destructive' : scoreDelta > 0 ? 'font-semibold' : 'text-muted-foreground')}>
                  {scoreDelta > 0 ? '▲ +' : scoreDelta < 0 ? '▼ −' : '= '}
                  {Math.abs(scoreDelta)} pts depuis l’audit #{previous.id}
                </p>
              )}
            </div>
          </div>
        )}
        <Stat label={running ? 'URL explorées' : 'URL'} value={c.pages} />
        {!running && (
          <>
            <Stat
              label="Erreurs"
              value={c.errors}
              alert
              onClick={() => update({ tab: null, r: 'overview' })}
              hint={
                delta && (
                  <span className={delta.tone === 'worse' ? 'font-semibold text-destructive' : delta.tone === 'better' ? 'font-semibold' : 'text-muted-foreground'}>
                    {delta.tone === 'worse' ? '▲' : delta.tone === 'better' ? '▼' : '='} {delta.text}
                  </span>
                )
              }
            />
            <Stat label="Avertissements" value={c.warnings} onClick={() => update({ tab: null, r: 'overview' })} />
          </>
        )}
      </div>

      {running && (
        <div className="panel flex flex-wrap items-center gap-x-4 gap-y-1 px-4 py-3 text-sm" role="status">
          <span className="font-semibold">
            {c.live?.paused ? 'Audit en pause' : c.live?.stopping ? 'Arrêt en cours…' : 'Audit en cours'}
          </span>
          <RunProgress pages={c.pages} live={c.live} className="order-last m-0 max-w-none" />
          {c.live && (
            <span className="text-muted-foreground">
              {c.live.urls_per_second.toFixed(1)} URL/s · file {c.live.queued} · {Math.round(c.live.avg_latency_ms)} ms · concurrence{' '}
              {c.live.concurrency}
              {c.live.failures > 0 && ` · ${c.live.failures} échec(s)`}
            </span>
          )}
          <span className="basis-full text-muted-foreground">
            Les URL explorées s’affichent au fil de l’eau ; problèmes, liens entrants et maillage sont calculés à la fin.
          </span>
        </div>
      )}
      <div className="flex min-h-0 flex-1 flex-col gap-4">
          <Tabs value={tab} onValueChange={(v) => update({ tab: v === 'data' ? null : String(v) })}>
            <TabsList>
              <TabsTrigger value="data">Données</TabsTrigger>
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
              <Workstation crawlId={id} projectId={c.project_id} running={running} onTab={(t) => update({ tab: t })} />
            </div>
          )}
          {tab !== 'data' && !running && (
            <div className="min-h-0 flex-1 lg:overflow-auto">
              {tab === 'links' && <LinksView crawlId={id} crawlStatus={c.status} />}
              {tab === 'diff' && <DiffView crawlId={id} projectId={c.project_id} />}
            </div>
          )}
      </div>
    </div>
  )
}
