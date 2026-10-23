import { ApiError } from '@/lib/api'

export function QueryError({ error }: { error: unknown }) {
  if (!error) return null
  const message = error instanceof Error ? error.message : String(error)
  const hint =
    error instanceof ApiError && error.status === 401
      ? ' — renseignez le jeton d’accès (bouton « Jeton » en haut à droite).'
      : error instanceof TypeError
        ? ' — l’API est-elle lancée ? (callistseo serve --db audits.db)'
        : ''
  return (
    <p role="alert" className="rounded-xl border border-destructive/40 bg-destructive-soft px-3 py-2 text-sm text-destructive">
      {message}
      {hint}
    </p>
  )
}
