import { useSearchParams } from 'react-router-dom'

/** Filtres et pagination vivent dans l'URL : liens partageables, bouton retour qui fonctionne. */
export function useQueryParams() {
  const [params, setParams] = useSearchParams()
  const update = (changes: Record<string, string | null>) =>
    setParams(
      (prev) => {
        const next = new URLSearchParams(prev)
        for (const [key, value] of Object.entries(changes)) {
          if (value === null || value === '') next.delete(key)
          else next.set(key, value)
        }
        return next
      },
      { replace: true },
    )
  return [params, update] as const
}
