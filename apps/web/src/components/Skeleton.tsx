import { cn } from '@/lib/utils'

/** Gabarit gris pendant le chargement : la page ne saute pas quand le contenu arrive. */
export function Skeleton({ className }: { className?: string }) {
  return <div aria-hidden className={cn('rounded-xl bg-muted motion-safe:animate-pulse', className)} />
}

export function PageSkeleton({ rows = 3 }: { rows?: number }) {
  return (
    <div role="status" aria-label="Chargement" className="grid gap-4">
      <Skeleton className="h-9 w-64" />
      {Array.from({ length: rows }, (_, i) => (
        <Skeleton key={i} className="h-24 w-full" />
      ))}
    </div>
  )
}
