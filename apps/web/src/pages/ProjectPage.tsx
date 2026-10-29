import { useState } from 'react'
import { Link, useNavigate, useParams } from 'react-router-dom'
import { useMutation, useQuery } from '@tanstack/react-query'
import { RunBadge } from '@/components/Badges'
import { DeleteProject } from '@/components/DeleteProject'
import { IntegrationsCard } from '@/components/IntegrationsCard'
import { ProjectConfigCard } from '@/components/ProjectConfigCard'
import { ScheduleCard } from '@/components/ScheduleCard'
import { SiteAuthCard } from '@/components/SiteAuthCard'
import { QueryError } from '@/components/QueryError'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { PageSkeleton } from '@/components/Skeleton'
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { HealthGauge, ScorePill, TrendChart } from '@/components/charts'
import { api } from '@/lib/api'
import { SCORE_LABEL, scoreLevel, trendPoints } from '@/lib/score'
import { formatDate, formatDuration } from '@/lib/format'
import { useDocumentTitle } from '@/lib/title'
import { useQueryParams } from '@/lib/useQueryParams'

export function ProjectPage() {
  const id = Number(useParams().id)
  const navigate = useNavigate()
  const [params, update] = useQueryParams()
  const tabParam = params.get('tab')
  const tab = tabParam === 'schedule' || tabParam === 'config' ? tabParam : 'audits'
  const [maxPages, setMaxPages] = useState('')
  const [delayMs, setDelayMs] = useState('')
  const project = useQuery({
    queryKey: ['project', id],
    queryFn: () => api.project(id),
    // Un audit en cours : on rafraîchit la liste pour voir son état changer.
    refetchInterval: (q) => (q.state.data?.crawls.some((c) => c.status === 'running') ? 2000 : false),
  })
  const start = useMutation({
    mutationFn: () =>
      api.startCrawl(id, { max_pages: Number(maxPages) || undefined, delay_ms: Number(delayMs) || undefined }),
    onSuccess: (crawl) => navigate(`/crawls/${crawl.id}`),
  })

  useDocumentTitle(project.data?.name)
  if (project.error) return <QueryError error={project.error} />
  if (!project.data) return <PageSkeleton />
  const p = project.data
  const points = trendPoints(p.crawls)
  // Dernier audit terminé avec un score (la liste est triée du plus récent au plus ancien).
  const latest = p.crawls.find((c): c is typeof c & { health: number } => c.health !== null)

  return (
    <div className="grid gap-6">
      <div>
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div className="min-w-0">
            <h1 className="text-3xl font-extrabold tracking-tight">{p.name}</h1>
            <p className="break-all text-sm text-muted-foreground">{p.start_url}</p>
          </div>
          <DeleteProject id={id} name={p.name} audits={p.crawls.length} running={p.crawls.some((c) => c.status === 'running')} />
        </div>
      </div>

      {/* Sur grand écran, la barre latérale porte déjà ces onglets. */}
      <Tabs className="lg:hidden" value={tab} onValueChange={(v) => update({ tab: v === 'audits' ? null : String(v) })}>
        <TabsList>
          <TabsTrigger value="audits">Audits</TabsTrigger>
          <TabsTrigger value="schedule">Planification</TabsTrigger>
          <TabsTrigger value="config">Configuration</TabsTrigger>
        </TabsList>
      </Tabs>

      {tab === 'schedule' && <ScheduleCard projectId={id} />}
      {tab === 'config' && (
        <div className="grid gap-4">
          <ProjectConfigCard projectId={id} />
          <SiteAuthCard projectId={id} />
          <IntegrationsCard projectId={id} />
        </div>
      )}

      {tab === 'audits' && latest && (
        <section className="panel-pop grid gap-4 p-4 md:grid-cols-[auto_minmax(0,1fr)] md:items-center" aria-label="Santé du site">
          <div className="flex items-center gap-4">
            <HealthGauge score={latest.health} />
            <div>
              <p className="text-xs font-medium text-muted-foreground">Score de santé</p>
              <p className="font-heading text-xl font-extrabold">{SCORE_LABEL[scoreLevel(latest.health)]}</p>
              <p className="text-xs text-muted-foreground">
                <Link to={`/crawls/${latest.id}`} className="underline underline-offset-4 hover:no-underline">
                  Audit #{latest.id}
                </Link>{' '}
                · {latest.errors.toLocaleString('fr-FR')} erreurs · {latest.pages.toLocaleString('fr-FR')} URL
              </p>
            </div>
          </div>
          <div className="grid gap-1">
            <h2 className="text-sm font-semibold">Évolution du score</h2>
            <TrendChart points={points} />
          </div>
        </section>
      )}

      {tab === 'audits' && (
      <Card>
        <CardHeader>
          <CardTitle>Lancer un audit</CardTitle>
          <CardDescription>
            Utilise la configuration du projet (onglet Configuration). Le serveur applique ses plafonds et respecte robots.txt, y compris son Crawl-delay.
          </CardDescription>
        </CardHeader>
        <CardContent>
          <form
            className="flex flex-wrap items-end gap-4"
            onSubmit={(e) => {
              e.preventDefault()
              start.mutate()
            }}
          >
            <div className="grid gap-1.5">
              <Label htmlFor="max-pages">Pages maximum</Label>
              <Input id="max-pages" type="number" min={1} value={maxPages} onChange={(e) => setMaxPages(e.target.value)} placeholder="configuration" className="w-36" />
            </div>
            <div className="grid gap-1.5">
              <Label htmlFor="delay-ms">Délai entre requêtes (ms)</Label>
              <Input id="delay-ms" type="number" min={0} step={50} value={delayMs} onChange={(e) => setDelayMs(e.target.value)} placeholder="configuration" className="w-40" />
            </div>
            <Button type="submit" disabled={start.isPending}>
              {start.isPending ? 'Lancement…' : 'Lancer l’audit'}
            </Button>
          </form>
          {start.error && (
            <div className="mt-3">
              <QueryError error={start.error} />
            </div>
          )}
        </CardContent>
      </Card>

      )}

      {tab === 'audits' && (
      <section className="grid gap-2">
        <h2 className="text-xl font-bold">Historique des audits</h2>
        {p.crawls.length === 0 ? (
          <p className="text-sm text-muted-foreground">Aucun audit pour ce projet.</p>
        ) : (
          <div className="table-panel">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Audit</TableHead>
                  <TableHead>Statut</TableHead>
                  <TableHead>Date</TableHead>
                  <TableHead>Durée</TableHead>
                  <TableHead>Score</TableHead>
                  <TableHead className="text-right">URL</TableHead>
                  <TableHead className="text-right">Erreurs</TableHead>
                  <TableHead className="text-right">Avertissements</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {p.crawls.map((c) => (
                  <TableRow key={c.id}>
                    <TableCell>
                      <Link to={`/crawls/${c.id}`} className="font-semibold underline underline-offset-4 hover:no-underline">
                        #{c.id}
                      </Link>
                    </TableCell>
                    <TableCell>
                      <RunBadge status={c.status} />
                    </TableCell>
                    <TableCell>{formatDate(c.started_at)}</TableCell>
                    <TableCell>{formatDuration(c.started_at, c.finished_at)}</TableCell>
                    <TableCell>{c.health !== null ? <ScorePill score={c.health} /> : <span className="text-muted-foreground">—</span>}</TableCell>
                    <TableCell className="text-right tabular-nums">{c.pages.toLocaleString('fr-FR')}</TableCell>
                    <TableCell className={`text-right tabular-nums ${c.errors > 0 ? 'font-semibold text-destructive' : ''}`}>{c.errors}</TableCell>
                    <TableCell className="text-right tabular-nums">{c.warnings.toLocaleString('fr-FR')}</TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </div>
        )}
      </section>
      )}
    </div>
  )
}
