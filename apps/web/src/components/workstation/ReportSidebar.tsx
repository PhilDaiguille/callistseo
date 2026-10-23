import { LayoutDashboard } from 'lucide-react'
import type { ReportsResponse } from '@/lib/api'
import { cn } from '@/lib/utils'

const DOT: Record<string, string> = {
  Error: 'bg-destructive',
  Warning: 'bg-warning-soft ring-1 ring-ink',
  Info: 'bg-info-soft ring-1 ring-ink',
}

function Item({
  label,
  count,
  active,
  dot,
  onClick,
}: {
  label: string
  count?: number
  active: boolean
  dot?: string
  onClick: () => void
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-current={active ? 'true' : undefined}
      className={cn(
        'flex w-full items-center gap-2 rounded-lg px-2 py-1.5 text-left text-sm transition-colors',
        active ? 'bg-primary font-semibold text-primary-foreground' : 'hover:bg-accent',
        count === 0 && !active && 'text-muted-foreground',
      )}
    >
      {dot && <span aria-hidden className={cn('size-2 shrink-0 rounded-full', dot)} />}
      <span className="min-w-0 flex-1 truncate">{label}</span>
      {count !== undefined && <span className="tabular-nums text-xs opacity-80">{count.toLocaleString('fr-FR')}</span>}
    </button>
  )
}

/** Barre latérale : vue d'ensemble, problèmes (gravité + nombre d'URL) puis rapports par groupe. */
export function ReportSidebar({
  reports,
  active,
  onSelect,
}: {
  reports: ReportsResponse
  active: string
  onSelect: (id: string) => void
}) {
  return (
    <nav aria-label="Rapports" className="grid content-start gap-3">
      <button
        type="button"
        onClick={() => onSelect('overview')}
        aria-current={active === 'overview' ? 'true' : undefined}
        className={cn(
          'flex items-center gap-2 rounded-lg px-2 py-1.5 text-sm font-semibold transition-colors',
          active === 'overview' ? 'bg-primary text-primary-foreground' : 'hover:bg-accent',
        )}
      >
        <LayoutDashboard className="size-4" aria-hidden /> À corriger en premier
      </button>

      {reports.issues.length > 0 && (
        <details open className="group">
          <summary className="cursor-pointer px-2 text-xs font-bold uppercase tracking-wide text-muted-foreground">
            Problèmes ({reports.issues.length})
          </summary>
          <div className="mt-1 grid gap-0.5">
            {reports.issues.map((i) => (
              <Item
                key={i.id}
                label={i.label}
                count={i.count}
                dot={DOT[i.severity]}
                active={active === i.id}
                onClick={() => onSelect(i.id)}
              />
            ))}
          </div>
        </details>
      )}

      {reports.groups.map((g, n) => (
        <details key={g.label} open={n < 2} className="group">
          <summary className="cursor-pointer px-2 text-xs font-bold uppercase tracking-wide text-muted-foreground">
            {g.label}
          </summary>
          <div className="mt-1 grid gap-0.5">
            {g.reports.map((r) => (
              <Item key={r.id} label={r.label} count={r.count} active={active === r.id} onClick={() => onSelect(r.id)} />
            ))}
          </div>
        </details>
      ))}
    </nav>
  )
}
