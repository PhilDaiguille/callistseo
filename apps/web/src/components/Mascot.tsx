import { cn } from '@/lib/utils'

type Mood = 'normal' | 'happy' | 'sleepy'

/**
 * Mascotte de CallistSEO : une petite araignée-robot qui explore (clin d'œil au « crawl »).
 * Dessin original, couleurs par jetons (suit le thème clair/sombre). Décorative par défaut.
 */
export function Mascot({
  size = 40,
  mood = 'normal',
  title,
  className,
}: {
  size?: number
  mood?: Mood
  /** Texte alternatif ; sans lui, l'illustration est ignorée des lecteurs d'écran. */
  title?: string
  className?: string
}) {
  const ink = 'var(--ink)'
  const leg = { stroke: ink, strokeWidth: 4, strokeLinecap: 'round' as const, fill: 'none' }
  return (
    <svg
      viewBox="0 0 100 100"
      width={size}
      height={size}
      role={title ? 'img' : undefined}
      aria-label={title}
      aria-hidden={title ? undefined : true}
      className={cn('shrink-0', className)}
    >
      {/* pattes */}
      <path d="M27 50 Q10 42 8 28" {...leg} />
      <path d="M24 58 Q8 56 5 70" {...leg} />
      <path d="M28 68 Q14 74 12 88" {...leg} />
      <path d="M73 50 Q90 42 92 28" {...leg} />
      <path d="M76 58 Q92 56 95 70" {...leg} />
      <path d="M72 68 Q86 74 88 88" {...leg} />
      {/* antenne */}
      <path d="M50 31 L50 17" {...leg} />
      <circle cx="50" cy="13" r="5.5" fill="var(--success)" stroke={ink} strokeWidth="3" />
      {/* corps */}
      <circle cx="50" cy="56" r="26" fill="var(--primary)" stroke={ink} strokeWidth="4" />
      {/* joues */}
      <circle cx="33" cy="65" r="3.8" fill="var(--destructive-soft)" />
      <circle cx="67" cy="65" r="3.8" fill="var(--destructive-soft)" />
      {mood === 'sleepy' ? (
        <>
          <path d="M34 55 Q41 61 48 55" {...leg} strokeWidth={3} />
          <path d="M52 55 Q59 61 66 55" {...leg} strokeWidth={3} />
          <path d="M46 68 H54" {...leg} strokeWidth={3} />
        </>
      ) : (
        <>
          <circle cx="41" cy="54" r="7.5" fill="var(--card)" stroke={ink} strokeWidth="3" />
          <circle cx="59" cy="54" r="7.5" fill="var(--card)" stroke={ink} strokeWidth="3" />
          <circle cx="42.5" cy="55" r="3.2" fill={ink} />
          <circle cx="57.5" cy="55" r="3.2" fill={ink} />
          <path d={mood === 'happy' ? 'M42 65 Q50 76 58 65' : 'M44 67 Q50 72 56 67'} {...leg} strokeWidth={3} />
        </>
      )}
    </svg>
  )
}
