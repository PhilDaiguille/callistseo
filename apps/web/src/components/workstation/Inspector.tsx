import { Link } from 'react-router-dom'
import { useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { X } from 'lucide-react'
import { HttpStatus, SeverityBadge } from '@/components/Badges'
import { LinkList } from '@/components/LinkList'
import { PageDetails } from '@/components/PageDetails'
import { QueryError } from '@/components/QueryError'
import { UrlLink } from '@/components/UrlLink'
import { Button } from '@/components/ui/button'
import { api } from '@/lib/api'
import { ruleInfo } from '@/lib/rules'
import { formatBytes } from '@/lib/table'
import { detailPath } from '@/lib/url'

function Fact({ label, value }: { label: string; value: string | number | null | undefined }) {
  if (value === null || value === undefined || value === '') return null
  return (
    <div className="flex justify-between gap-3 text-sm">
      <dt className="text-muted-foreground">{label}</dt>
      <dd className="truncate text-right tabular-nums">{value}</dd>
    </div>
  )
}

/** Volet de détail d'une URL : réponse, contenu, problèmes, liens. */
const LINKS_SHOWN = 10

export function Inspector({ crawlId, url, onClose }: { crawlId: number; url: string; onClose: () => void }) {
  const [dir, setDir] = useState<'in' | 'out'>('in')
  const detail = useQuery({ queryKey: ['page-detail', crawlId, url], queryFn: () => api.pageDetail(crawlId, url) })
  const body = (() => {
    if (detail.error) return <QueryError error={detail.error} />
    if (!detail.data) return <p className="text-sm text-muted-foreground">Chargement…</p>
    const { page, issues, inlinks, outlinks } = detail.data
    const f = page.facts
    return (
      <div className="grid gap-4">
        <div className="flex items-start gap-2">
          <HttpStatus status={page.status} error={page.error} />
          <UrlLink url={page.url} className="flex-1" />
        </div>

        <section className="grid gap-1">
          <h3 className="text-sm font-bold">Problèmes ({issues.length})</h3>
          {issues.length === 0 && <p className="text-sm text-muted-foreground">Aucun problème détecté.</p>}
          <ul className="grid gap-1.5">
            {issues.map((i, n) => (
              <li key={`${i.rule}|${n}`} className="rounded-lg border border-line p-2">
                <div className="flex flex-wrap items-center gap-1.5 text-sm">
                  <SeverityBadge severity={i.severity} />
                  <span className="font-medium">{ruleInfo(i.rule).label}</span>
                </div>
                <p className="mt-1 text-xs text-muted-foreground">{i.message}</p>
              </li>
            ))}
          </ul>
        </section>

        <section className="grid gap-1">
          <h3 className="text-sm font-bold">Réponse et contenu</h3>
          {f && f.ttfb_ms !== null ? (
            <dl className="grid gap-0.5">
              <Fact label="Premier octet" value={f.ttfb_ms !== null ? `${f.ttfb_ms} ms` : null} />
              <Fact label="Total" value={f.total_ms !== null ? `${f.total_ms} ms` : null} />
              <Fact label="Taille" value={f.size !== null ? formatBytes(f.size) : null} />
              <Fact label="Type" value={f.content_type} />
              <Fact label="Mots" value={f.word_count} />
              <Fact label="Langue" value={f.lang} />
              <Fact label="Images" value={f.images_total ? `${f.images_total} (${f.images.filter(([, a]) => a === null).length} sans alt)` : null} />
              <Fact label="hreflang" value={f.hreflang.length || null} />
              <Fact label="JSON-LD" value={f.json_ld.length || null} />
              <Fact label="Liens nofollow" value={f.nofollow_links || null} />
            </dl>
          ) : (
            <p className="text-sm text-muted-foreground">Mesures indisponibles (audit antérieur à cette version).</p>
          )}
          {f && f.headers.length > 0 && (
            <details className="text-xs">
              <summary className="cursor-pointer text-muted-foreground">En-têtes conservés ({f.headers.length})</summary>
              <dl className="mt-1 grid gap-0.5 font-mono">
                {f.headers.map(([k, v]) => (
                  <div key={k} className="break-all">
                    <dt className="inline font-bold">{k}:</dt> <dd className="inline">{v}</dd>
                  </div>
                ))}
              </dl>
            </details>
          )}
        </section>

        <section className="grid gap-1">
          <h3 className="text-sm font-bold">Métadonnées</h3>
          <PageDetails page={page} />
        </section>

        <section className="grid gap-2 text-sm">
          <h3 className="text-sm font-bold">Liens</h3>
          <div role="tablist" aria-label="Sens des liens" className="flex gap-1">
            {(['in', 'out'] as const).map((k) => (
              <button
                key={k}
                type="button"
                role="tab"
                aria-selected={dir === k}
                onClick={() => setDir(k)}
                className={`rounded-lg px-2.5 py-1 text-sm font-semibold ${dir === k ? 'bg-accent' : 'text-muted-foreground hover:bg-accent/50'}`}
              >
                {k === 'in' ? `Entrants (${inlinks.total})` : `Sortants (${outlinks.total})`}
              </button>
            ))}
          </div>
          {dir === 'in' ? (
            <LinkList crawlId={crawlId} total={inlinks.total} limit={LINKS_SHOWN} empty="Aucune autre page crawlée ne mène ici." items={inlinks.items.map((i) => ({ ...i, crawled: true }))} />
          ) : (
            <LinkList crawlId={crawlId} total={outlinks.total} limit={LINKS_SHOWN} empty="Aucun lien interne sur cette page." items={outlinks.items} />
          )}
          {(inlinks.total > LINKS_SHOWN || outlinks.total > LINKS_SHOWN) && <p className="text-xs text-muted-foreground">{LINKS_SHOWN} premiers affichés.</p>}
          <Link to={detailPath(crawlId, url)} className="underline underline-offset-4">
            Ouvrir la fiche complète (filtres, liens cassés)
          </Link>
        </section>
      </div>
    )
  })()

  return (
    <aside aria-label="Détail de l'URL" className="panel flex h-full min-h-0 flex-col">
      <div className="flex items-center justify-between border-b border-line px-3 py-2">
        <h2 className="text-sm font-bold">Inspecteur</h2>
        <Button variant="ghost" size="icon-sm" aria-label="Fermer l'inspecteur" onClick={onClose}>
          <X aria-hidden />
        </Button>
      </div>
      <div className="min-h-0 flex-1 overflow-auto p-3">{body}</div>
    </aside>
  )
}
