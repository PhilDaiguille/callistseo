import { cn } from '@/lib/utils'

/** Bloc de chiffre clé (niveau léger). `alert` le met en avant quand la valeur est non nulle. */
export function Stat({ label, value, alert }: { label: string; value: number; alert?: boolean }) {
  return (
    <div className={cn('panel px-4 py-2', alert && value > 0 && 'border-destructive/50 bg-destructive-soft')}>
      <p className="text-xs text-muted-foreground">{label}</p>
      <p className="font-heading text-2xl font-extrabold tabular-nums">{value}</p>
    </div>
  )
}
