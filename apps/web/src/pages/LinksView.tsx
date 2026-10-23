import { useQuery } from '@tanstack/react-query'
import { Stat } from '@/components/Stat'
import { QueryError } from '@/components/QueryError'
import { UrlLink } from '@/components/UrlLink'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { api } from '@/lib/api'

function Ranking({ crawlId, title, unit, rows }: { crawlId: number; title: string; unit: string; rows: { url: string; count: number }[] }) {
  return (
    <section className="grid content-start gap-2">
      <h3 className="text-lg font-bold">{title}</h3>
      <div className="table-panel">
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>URL</TableHead>
              <TableHead className="w-28 text-right">{unit}</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {rows.map((r) => (
              <TableRow key={r.url}>
                <TableCell className="max-w-md">
                  <UrlLink url={r.url} crawlId={crawlId} />
                </TableCell>
                <TableCell className="text-right tabular-nums">{r.count}</TableCell>
              </TableRow>
            ))}
            {rows.length === 0 && (
              <TableRow>
                <TableCell colSpan={2} className="py-6 text-center text-muted-foreground">
                  Aucun lien interne.
                </TableCell>
              </TableRow>
            )}
          </TableBody>
        </Table>
      </div>
    </section>
  )
}

/** Maillage interne : répartition par profondeur, pages les plus liées, pages orphelines. */
export function LinksView({ crawlId, crawlStatus }: { crawlId: number; crawlStatus: string }) {
  const links = useQuery({ queryKey: ['links', crawlId, crawlStatus], queryFn: () => api.links(crawlId) })
  if (links.error) return <QueryError error={links.error} />
  if (!links.data) return <p className="text-sm text-muted-foreground">Chargement…</p>
  const l = links.data
  const maxCount = Math.max(1, ...l.depths.map((d) => d.count))

  return (
    <div className="grid gap-6">
      <div className="flex flex-wrap gap-3">
        <Stat label="Liens internes" value={l.internal_links} />
        <Stat label="Pages orphelines" value={l.orphans.total} />
        <Stat label="Profondeur maximale" value={l.depths.at(-1)?.depth ?? 0} />
      </div>

      <section className="grid gap-2">
        <h3 className="text-lg font-bold">Pages par profondeur</h3>
        <ul className="grid gap-1.5 panel px-4 py-3">
          {l.depths.map((d) => (
            <li key={d.depth} className="grid grid-cols-[5rem_1fr_3rem] items-center gap-3 text-sm">
              <span className="text-muted-foreground">Niveau {d.depth}</span>
              <span className="h-3 rounded-sm bg-muted" role="presentation">
                <span className="block h-full rounded-sm bg-foreground/70" style={{ width: `${(d.count / maxCount) * 100}%` }} />
              </span>
              <span className="text-right tabular-nums">{d.count}</span>
            </li>
          ))}
        </ul>
      </section>

      <div className="grid gap-6 lg:grid-cols-2">
        <Ranking crawlId={crawlId} title="Pages les plus liées" unit="Pages liantes" rows={l.top_linked} />
        <Ranking crawlId={crawlId} title="Pages qui lient le plus" unit="Pages ciblées" rows={l.top_linking} />
      </div>

      <section className="grid gap-2">
        <h3 className="text-lg font-bold">
          Pages orphelines <span className="text-muted-foreground tabular-nums">({l.orphans.total})</span>
        </h3>
        {l.orphans.total === 0 ? (
          <p className="text-sm text-muted-foreground">Chaque page (hors accueil) reçoit au moins un lien d'une autre page crawlée.</p>
        ) : (
          <>
            <p className="text-sm text-muted-foreground">
              Aucune autre page crawlée ne lie vers elles : elles ont été trouvées via le sitemap uniquement. Les redirections ne comptent pas comme des liens.
            </p>
            <ul className="max-h-64 overflow-auto panel px-3 py-2">
              {l.orphans.items.map((u) => (
                <li key={u} className="py-0.5">
                  <UrlLink url={u} crawlId={crawlId} />
                </li>
              ))}
            </ul>
            {l.orphans.total > l.orphans.items.length && (
              <p className="text-xs text-muted-foreground">Les {l.orphans.items.length} premières sont affichées.</p>
            )}
          </>
        )}
      </section>
    </div>
  )
}
