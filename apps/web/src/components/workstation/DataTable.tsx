import { useEffect, useRef } from 'react'
import { useInfiniteQuery } from '@tanstack/react-query'
import { useVirtualizer } from '@tanstack/react-virtual'
import { ArrowDown, ArrowUp } from 'lucide-react'
import { HttpStatus } from '@/components/Badges'
import { QueryError } from '@/components/QueryError'
import { api, type Cell, type ColumnDef } from '@/lib/api'
import { UrlLink } from '@/components/UrlLink'
import { columnTemplate, formatCell, wrap, widthOf } from '@/lib/table'
import { cn } from '@/lib/utils'

const PAGE = 200
const ROW = 40

export interface TableState {
  report: string
  columns: string[]
  sort: string | null
  dir: 'asc' | 'desc'
  search: string
  segment: number | null
}

function CellView({ id, value, row, cols }: { id: string; value: Cell; row: Cell[]; cols: string[] }) {
  if (id === 'status') {
    const err = cols.includes('error') ? (row[cols.indexOf('error')] as string | null) : null
    return <HttpStatus status={value as number | null} error={err} />
  }
  if (id === 'url' && typeof value === 'string') return <UrlLink url={value} compact />
  return <span title={typeof value === 'string' ? value : undefined}>{formatCell(id, value)}</span>
}

/**
 * Tableau virtualisé : seules ~30 lignes sont dans le DOM, même avec 100 000 URL. Les lignes sont
 * chargées par tranches de 200 quand on approche de la fin. Tri côté serveur (liste blanche).
 */
export function DataTable({
  crawlId,
  state,
  defs,
  selected,
  onSelect,
  onSort,
}: {
  crawlId: number
  state: TableState
  defs: ColumnDef[]
  selected: string | null
  onSelect: (url: string) => void
  onSort: (column: string) => void
}) {
  const { report, columns, sort, dir, search, segment } = state
  const query = useInfiniteQuery({
    queryKey: ['rows', crawlId, report, columns.join(','), sort, dir, search, segment],
    queryFn: ({ pageParam }) =>
      api.rows(crawlId, { report, columns, sort: sort ?? undefined, dir, search, segment: segment ?? undefined, limit: PAGE, offset: pageParam }),
    initialPageParam: 0,
    getNextPageParam: (last) => (last.offset + last.limit < last.total ? last.offset + last.limit : undefined),
    placeholderData: (prev) => prev,
  })
  const rows = query.data?.pages.flatMap((p) => p.rows) ?? []
  const total = query.data?.pages[0]?.total ?? 0
  const urlIdx = columns.indexOf('url')

  const scroller = useRef<HTMLDivElement>(null)
  const virtual = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scroller.current,
    estimateSize: () => ROW,
    overscan: 12,
  })
  const items = virtual.getVirtualItems()
  const lastIndex = items.at(-1)?.index ?? 0
  const { hasNextPage, isFetchingNextPage, fetchNextPage } = query
  useEffect(() => {
    if (hasNextPage && !isFetchingNextPage && lastIndex >= rows.length - 30) void fetchNextPage()
  }, [hasNextPage, isFetchingNextPage, fetchNextPage, lastIndex, rows.length])

  const widths = columns.map(widthOf)
  const template = columnTemplate(columns)
  const totalWidth = widths.reduce((a, b) => a + b, 0) + 16

  const move = (delta: number) => {
    if (urlIdx < 0 || rows.length === 0) return
    const current = rows.findIndex((r) => r[urlIdx] === selected)
    const next = wrap(rows.length, current < 0 ? (delta > 0 ? -1 : 0) : current, delta)
    virtual.scrollToIndex(next)
    onSelect(rows[next]![urlIdx] as string)
  }

  if (query.error) return <QueryError error={query.error} />
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="px-1 pb-1 text-xs text-muted-foreground" aria-live="polite">
        {query.isPending ? 'Chargement…' : `${total.toLocaleString('fr-FR')} URL`}
        {search && ` · recherche « ${search} »`}
      </div>
      <div
        ref={scroller}
        role="grid"
        aria-rowcount={total}
        aria-label="URL du rapport"
        tabIndex={0}
        onKeyDown={(e) => {
          if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
            e.preventDefault()
            move(e.key === 'ArrowDown' ? 1 : -1)
          }
        }}
        className="panel min-h-0 flex-1 overflow-auto focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring"
      >
        <div style={{ width: totalWidth, minWidth: '100%' }}>
          <div
            role="row"
            className="sticky top-0 z-10 grid border-b border-line bg-card px-2 text-xs font-bold"
            style={{ gridTemplateColumns: template }}
          >
            {columns.map((c) => {
              const def = defs.find((d) => d.id === c)
              const active = sort === c
              return (
                <button
                  key={c}
                  type="button"
                  role="columnheader"
                  aria-sort={active ? (dir === 'asc' ? 'ascending' : 'descending') : 'none'}
                  onClick={() => onSort(c)}
                  className={cn('flex items-center gap-1 truncate py-2 pr-2 text-left hover:text-foreground', def?.numeric && c !== 'status' && 'justify-end text-right')}
                >
                  <span className="truncate">{def?.label ?? c}</span>
                  {active && (dir === 'asc' ? <ArrowUp className="size-3" aria-hidden /> : <ArrowDown className="size-3" aria-hidden />)}
                </button>
              )
            })}
          </div>
          {rows.length === 0 && !query.isPending && (
            <p className="px-4 py-8 text-center text-sm text-muted-foreground">Aucune URL dans ce rapport.</p>
          )}
          <div style={{ height: virtual.getTotalSize(), position: 'relative' }}>
            {items.map((vi) => {
              const row = rows[vi.index]!
              const url = urlIdx >= 0 ? (row[urlIdx] as string) : String(vi.index)
              const on = url === selected
              return (
                <div
                  key={vi.key}
                  role="row"
                  aria-selected={on}
                  onClick={() => onSelect(url)}
                  className={cn(
                    'absolute left-0 grid w-full cursor-pointer items-center border-b border-line/60 px-2 text-sm',
                    on ? 'bg-accent' : 'hover:bg-accent/50',
                  )}
                  style={{ height: ROW, transform: `translateY(${vi.start}px)`, gridTemplateColumns: template }}
                >
                  {columns.map((c, i) => (
                    <div
                      key={c}
                      role="gridcell"
                      className={cn('truncate pr-2', defs.find((d) => d.id === c)?.numeric && c !== 'status' && 'text-right tabular-nums')}
                    >
                      <CellView id={c} value={row[i] ?? null} row={row} cols={columns} />
                    </div>
                  ))}
                </div>
              )
            })}
          </div>
        </div>
      </div>
    </div>
  )
}
