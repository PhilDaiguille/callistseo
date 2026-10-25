import type { LiveStats } from '@/lib/api'
import { formatEta, runProgress } from '@/lib/progress'
import { cn } from '@/lib/utils'

/** Barre d'avancement d'un audit en cours : pourcentage, URL terminées / découvertes, débit et durée restante. */
export function RunProgress({ pages, live }: { pages: number; live?: LiveStats }) {
  const p = runProgress(pages, live)
  const idle = live?.paused || live?.stopping
  return (
    <div className="mx-auto mt-4 grid w-full max-w-xl gap-1.5 text-left">
      <div className="flex items-baseline justify-between text-sm">
        <span className="font-semibold tabular-nums">{p.percent} %</span>
        <span className="text-muted-foreground tabular-nums">
          {p.done.toLocaleString('fr-FR')} / {p.total.toLocaleString('fr-FR')} URL
        </span>
      </div>
      <div
        role="progressbar"
        aria-label="Avancement de l'audit"
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={p.percent}
        className="h-3 overflow-hidden rounded-full bg-muted"
      >
        <div
          className={cn('h-full rounded-full bg-primary transition-[width] duration-700 ease-out', idle && 'opacity-50')}
          style={{ width: `${Math.max(p.percent, p.done > 0 ? 2 : 0)}%` }}
        />
      </div>
      <p className="text-xs text-muted-foreground">
        Le total grandit tant que de nouvelles URL sont découvertes.
        {p.etaSeconds !== null && ` Environ ${formatEta(p.etaSeconds)} restantes.`}
      </p>
    </div>
  )
}
