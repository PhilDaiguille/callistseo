import type { ReactNode } from 'react'
import { Mascot } from '@/components/Mascot'
import { cn } from '@/lib/utils'

/** État vide ou d'attente : la mascotte porte l'humeur (contente, endormie, au travail). */
export function EmptyState({
  mood = 'normal',
  working,
  children,
}: {
  mood?: 'normal' | 'happy' | 'sleepy'
  /** Audit en cours : la mascotte « respire » (désactivé si l'utilisateur réduit les animations). */
  working?: boolean
  children: ReactNode
}) {
  return (
    <div className="panel flex flex-col items-center gap-3 px-4 py-8 text-center">
      <Mascot size={72} mood={mood} className={cn(working && 'motion-safe:animate-pulse')} />
      <p className="max-w-prose text-sm text-muted-foreground">{children}</p>
    </div>
  )
}
