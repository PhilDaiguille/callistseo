import { Link, useParams } from 'react-router-dom'
import { useQuery } from '@tanstack/react-query'
import { HttpStatus, SeverityBadge } from '@/components/Badges'
import { LinkList } from '@/components/LinkList'
import { PageDetails } from '@/components/PageDetails'
import { QueryError } from '@/components/QueryError'
import { UrlLink } from '@/components/UrlLink'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { api } from '@/lib/api'
import { ruleInfo } from '@/lib/rules'
import { useQueryParams } from '@/lib/useQueryParams'

/** Détail d'une URL de l'audit : métadonnées, problèmes, pages qui y mènent et liens sortants. */
export function UrlDetailPage() {
  const crawlId = Number(useParams().id)
  const [params] = useQueryParams()
  const url = params.get('u') ?? ''
  const detail = useQuery({
    queryKey: ['page-detail', crawlId, url],
    queryFn: () => api.pageDetail(crawlId, url),
    enabled: url !== '',
  })

  const back = (
    <Link to={`/crawls/${crawlId}`} className="text-sm text-muted-foreground hover:text-foreground">
      ← Audit #{crawlId}
    </Link>
  )
  if (!url) return <div className="grid gap-3">{back}<p className="text-sm text-muted-foreground">Aucune URL demandée.</p></div>
  if (detail.error) return <div className="grid gap-3">{back}<QueryError error={detail.error} /></div>
  if (!detail.data) return <p className="text-sm text-muted-foreground">Chargement…</p>
  const { page, issues, inlinks, outlinks } = detail.data

  return (
    <div className="grid gap-6">
      <div>
        {back}
        <div className="mt-1 flex flex-wrap items-center gap-3">
          <HttpStatus status={page.status} error={page.error} />
          <h1 className="min-w-0 flex-1 text-lg font-semibold">
            <UrlLink url={page.url} className="[&_a]:text-lg [&_span]:text-lg" />
          </h1>
        </div>
        <p className="mt-1 text-sm text-muted-foreground">Profondeur {page.depth}</p>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>Métadonnées</CardTitle>
        </CardHeader>
        <CardContent>
          <PageDetails page={page} />
        </CardContent>
      </Card>

      <section className="grid gap-2">
        <h2 className="text-xl font-bold">
          Problèmes <span className="text-muted-foreground tabular-nums">({issues.length})</span>
        </h2>
        {issues.length === 0 ? (
          <p className="text-sm text-muted-foreground">Aucun problème détecté sur cette URL.</p>
        ) : (
          <ul className="grid gap-2">
            {issues.map((i, n) => (
              <li key={`${i.rule}|${n}`} className="panel px-3 py-2">
                <div className="flex flex-wrap items-center gap-2">
                  <SeverityBadge severity={i.severity} />
                  <span className="text-sm font-medium">{ruleInfo(i.rule).label}</span>
                  <span className="text-sm text-muted-foreground">— {i.message}</span>
                </div>
                <p className="mt-1 text-xs text-muted-foreground">{ruleInfo(i.rule).help}</p>
              </li>
            ))}
          </ul>
        )}
      </section>

      <div className="grid gap-6 lg:grid-cols-2">
        <section className="grid content-start gap-2">
          <h2 className="text-xl font-bold">
            Liens entrants <span className="text-muted-foreground tabular-nums">({inlinks.total})</span>
          </h2>
          <LinkList crawlId={crawlId} total={inlinks.total} empty="Aucune autre page crawlée ne mène ici." items={inlinks.items.map((i) => ({ ...i, crawled: true }))} />
        </section>

        <section className="grid content-start gap-2">
          <h2 className="text-xl font-bold">
            Liens sortants <span className="text-muted-foreground tabular-nums">({outlinks.total})</span>
          </h2>
          <LinkList crawlId={crawlId} total={outlinks.total} empty="Cette page ne contient aucun lien interne." items={outlinks.items} />
        </section>
      </div>
    </div>
  )
}
