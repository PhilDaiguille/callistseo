/** Fiche détaillée d'une URL dans un audit. */
export const detailPath = (crawlId: number, url: string): string => `/crawls/${crawlId}/url?u=${encodeURIComponent(url)}`

/** Chemin et paramètres d'une URL (« /p/12?x=1 ») : lisible dans un volet étroit ; l'URL entière sinon. */
export function shortUrl(url: string): string {
  try {
    const u = new URL(url)
    return `${u.pathname}${u.search}` || url
  } catch {
    return url
  }
}

/** Seules les adresses http(s) sont ouvertes hors de l'application (jamais `javascript:` ni `file:`). */
export function isWebUrl(url: string): boolean {
  try {
    const { protocol } = new URL(url)
    return protocol === 'http:' || protocol === 'https:'
  } catch {
    return false
  }
}

export interface LinkItem {
  url: string
  status: number | null
  /** Page explorée dans cet audit (un lien entrant l'est toujours). */
  crawled: boolean
  title?: string | null
}

/** Lien à corriger : réponse 4xx/5xx ou page explorée sans réponse (erreur réseau). Une page non explorée n'est pas « cassée ». */
export const isBrokenLink = (l: LinkItem): boolean => l.crawled && (l.status === null || l.status >= 400)
