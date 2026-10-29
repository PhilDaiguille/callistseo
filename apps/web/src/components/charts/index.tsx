import { lazy, Suspense } from 'react'
import { Skeleton } from '@/components/Skeleton'
import type { Severity } from '@/lib/api'
import { SEVERITY_LABEL } from '@/lib/rules'
import { SCORE_LABEL, scoreColor, scoreLevel } from '@/lib/score'
import { cn } from '@/lib/utils'

const GaugeRing = lazy(() => import('./Charts').then((m) => ({ default: m.GaugeRing })))
const TrendLines = lazy(() => import('./Charts').then((m) => ({ default: m.TrendLines })))

/** Jauge du score de santé : anneau (Recharts) et chiffre écrit en clair au centre. */
export function HealthGauge({ score, size = 112 }: { score: number; size?: number }) {
  return (
    <div className="relative shrink-0" style={{ width: size, height: size }} role="img" aria-label={`Score de santé : ${score} sur 100 (${SCORE_LABEL[scoreLevel(score)]})`}>
      <div aria-hidden>
        <Suspense fallback={<Skeleton className="size-full rounded-full" />}>
          <GaugeRing score={score} size={size} />
        </Suspense>
      </div>
      <div aria-hidden className="absolute inset-0 grid place-content-center text-center">
        <span className="font-heading font-extrabold tabular-nums leading-none" style={{ fontSize: size * 0.3 }}>
          {score}
        </span>
        {size >= 96 && <span className="text-xs text-muted-foreground">/ 100</span>}
      </div>
    </div>
  )
}

/** Pastille compacte du score (listes) : sans Recharts. */
export function ScorePill({ score, className }: { score: number; className?: string }) {
  return (
    <span className={cn('inline-flex items-center gap-1.5 rounded-full border border-line px-2 py-0.5 text-xs font-semibold tabular-nums', className)} title={`Score de santé : ${SCORE_LABEL[scoreLevel(score)]}`}>
      <span aria-hidden className="size-2 rounded-full" style={{ background: scoreColor(score) }} />
      <span>{score}</span>
      <span className="sr-only">sur 100, score de santé</span>
    </span>
  )
}

// Pastels de la charte : rouge doux, ambre, lavande (`--primary`).
const SEVERITY_COLOR: Record<Severity, string> = { Error: 'var(--score-bad)', Warning: 'var(--score-mid)', Info: 'var(--primary)' }

// ponytail: barre empilée en flex, Recharts n'apporte rien pour trois segments.
/** Répartition des URL concernées par gravité, avec la légende chiffrée. */
export function SeverityBar({ totals }: { totals: Record<Severity, number> }) {
  const order: Severity[] = ['Error', 'Warning', 'Info']
  const sum = order.reduce((n, s) => n + totals[s], 0)
  return (
    <div className="grid gap-2">
      <div aria-hidden className="flex h-2.5 overflow-hidden rounded-full bg-muted">
        {sum > 0 && order.map((s) => totals[s] > 0 && <div key={s} style={{ width: `${(totals[s] / sum) * 100}%`, background: SEVERITY_COLOR[s] }} />)}
      </div>
      <ul className="flex flex-wrap gap-x-4 gap-y-1 text-sm">
        {order.map((s) => (
          <li key={s} className="flex items-center gap-1.5">
            <span aria-hidden className="size-2.5 rounded-full" style={{ background: SEVERITY_COLOR[s] }} />
            <span className="text-muted-foreground">{SEVERITY_LABEL[s]}</span>
            <b className="tabular-nums">{totals[s].toLocaleString('fr-FR')}</b>
          </li>
        ))}
      </ul>
    </div>
  )
}

/** Courbe du score sur les audits terminés ; rien à tracer sous deux points. */
export function TrendChart({ points, className }: { points: { id: number; health: number; errors: number }[]; className?: string }) {
  if (points.length < 2) return <p className="text-sm text-muted-foreground">La tendance apparaît à partir de deux audits terminés.</p>
  const first = points[0]
  const last = points[points.length - 1]
  return (
    <figure className={cn('h-44', className)}>
      <div aria-hidden className="size-full">
        <Suspense fallback={<Skeleton className="size-full" />}>
          <TrendLines points={points} />
        </Suspense>
      </div>
      <figcaption className="sr-only">
        Score de santé de {first.health} (audit #{first.id}) à {last.health} (audit #{last.id}) sur {points.length} audits.
      </figcaption>
    </figure>
  )
}
