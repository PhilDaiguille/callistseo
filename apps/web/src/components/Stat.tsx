import type { ReactNode } from 'react'
import { cn } from '@/lib/utils'

/**
 * Carte d'indicateur (niveau fort). `alert` la teinte quand la valeur est non nulle ; `hint` ajoute
 * une ligne (évolution…) ; `onClick` en fait un bouton (ex. ouvrir les problèmes).
 */
export function Stat({
  label,
  value,
  alert,
  hint,
  onClick,
  className,
}: {
  label: string
  value: number
  alert?: boolean
  hint?: ReactNode
  onClick?: () => void
  className?: string
}) {
  const cls = cn('panel-pop flex flex-col justify-start px-4 py-3 text-left', alert && value > 0 && 'border-destructive/40 bg-destructive-soft', onClick && 'transition-colors hover:bg-accent', className)
  const body = (
    <>
      <p className="text-xs font-medium text-muted-foreground">{label}</p>
      <p className="font-heading text-2xl font-extrabold tabular-nums">{value.toLocaleString('fr-FR')}</p>
      {hint && <p className="mt-0.5 text-xs">{hint}</p>}
    </>
  )
  return onClick ? (
    <button type="button" onClick={onClick} className={cls}>
      {body}
    </button>
  ) : (
    <div className={cls}>{body}</div>
  )
}
