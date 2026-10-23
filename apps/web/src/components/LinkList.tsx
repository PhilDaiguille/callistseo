import { useState } from 'react'
import { HttpStatus } from '@/components/Badges'
import { CopyButton } from '@/components/CopyButton'
import { UrlLink } from '@/components/UrlLink'
import { Input } from '@/components/ui/input'
import { matches } from '@/lib/table'
import { isBrokenLink, type LinkItem } from '@/lib/url'

/**
 * Liste de liens d'une URL (entrants ou sortants) : statut, URL cliquable, ouverture et copie.
 * `limit` : liste courte (inspecteur), sans filtre. Sans `limit` : filtre texte, « cassés seulement »
 * et copie de toute la liste (pour la donner à un agent IA par exemple).
 */
export function LinkList({
  crawlId,
  items,
  total,
  limit,
  empty,
}: {
  crawlId: number
  items: LinkItem[]
  /** Total réel, quand le serveur a plafonné la liste. */
  total: number
  limit?: number
  empty: string
}) {
  const [query, setQuery] = useState('')
  const [brokenOnly, setBrokenOnly] = useState(false)
  if (total === 0) return <p className="text-sm text-muted-foreground">{empty}</p>

  const broken = items.filter(isBrokenLink).length
  const shown = limit
    ? items.slice(0, limit)
    : items.filter((l) => (!brokenOnly || isBrokenLink(l)) && matches(query, `${l.url} ${l.title ?? ''}`))

  return (
    <div className="grid gap-2">
      {!limit && (
        <div className="flex flex-wrap items-center gap-2">
          <Input type="search" value={query} onChange={(e) => setQuery(e.target.value)} placeholder="Filtrer par URL ou title…" aria-label="Filtrer les liens" className="min-w-40 flex-1" />
          {broken > 0 && (
            <label className="flex cursor-pointer items-center gap-1.5 text-sm">
              <input type="checkbox" checked={brokenOnly} onChange={(e) => setBrokenOnly(e.target.checked)} />
              Cassés seulement ({broken})
            </label>
          )}
          <CopyButton text={shown.map((l) => l.url).join('\n')} label="Copier la liste affichée" />
        </div>
      )}
      <ul className={limit ? 'grid gap-0.5' : 'panel max-h-[32rem] overflow-auto px-2 py-1'}>
        {shown.map((l, n) => (
          <li key={`${l.url}|${n}`} className="flex items-center gap-2 py-0.5">
            <span className="w-14 shrink-0">
              {l.crawled ? <HttpStatus status={l.status} /> : <span className="text-xs text-muted-foreground">non explorée</span>}
            </span>
            <UrlLink url={l.url} crawlId={l.crawled ? crawlId : undefined} short={!!limit} className="flex-1" />
          </li>
        ))}
        {shown.length === 0 && <li className="py-2 text-sm text-muted-foreground">Aucun lien ne correspond.</li>}
      </ul>
      {!limit && total > items.length && (
        <p className="text-xs text-muted-foreground">
          Les {items.length.toLocaleString('fr-FR')} premiers sur {total.toLocaleString('fr-FR')} sont affichés.
        </p>
      )}
    </div>
  )
}
