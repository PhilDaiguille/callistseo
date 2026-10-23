import { useEffect } from 'react'

/** Titre d'onglet par page : on retrouve le bon onglet, et l'historique du navigateur est lisible. */
export function useDocumentTitle(title: string | null | undefined): void {
  useEffect(() => {
    document.title = title ? `${title} · CallistSEO` : 'CallistSEO'
  }, [title])
}
