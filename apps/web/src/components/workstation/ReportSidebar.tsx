import type { ReportsResponse, Severity } from '@/lib/api'
import { SEVERITY_COLOR } from '@/lib/score'
import { cn } from '@/lib/utils'

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
  dot?: Severity
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
      {dot && <span aria-hidden className="size-2 shrink-0 rounded-full" style={{ background: SEVERITY_COLOR[dot] }} />}
      <span className="min-w-0 flex-1 truncate">{label}</span>
      {count !== undefined && <span className="tabular-nums text-xs opacity-80">{count.toLocaleString('fr-FR')}</span>}
    </button>
  )
}

/** Colonne de l'explorateur : problèmes (gravité + nombre d'URL) puis rapports par groupe. */
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
      {reports.issues.length > 0 && (
        <details open className="group">
          <summary className="cursor-pointer px-2 text-xs font-semibold text-muted-foreground">
            Problèmes ({reports.issues.length})
          </summary>
          <div className="mt-1 grid gap-0.5">
            {reports.issues.map((i) => (
              <Item
                key={i.id}
                label={i.label}
                count={i.count}
                dot={i.severity}
                active={active === i.id}
                onClick={() => onSelect(i.id)}
              />
            ))}
          </div>
        </details>
      )}

      {reports.groups.map((g, n) => (
        <details key={g.label} open={n < 2} className="group">
          <summary className="cursor-pointer px-2 text-xs font-semibold text-muted-foreground">
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
