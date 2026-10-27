/** Une ligne `Nom: valeur` par en-tête ; lignes vides ignorées. Renvoie l'erreur de la première ligne invalide. */
export function parseHeaders(text: string): { headers: { name: string; value: string }[]; error: string | null } {
  const headers: { name: string; value: string }[] = []
  for (const [i, raw] of text.split('\n').entries()) {
    const line = raw.trim()
    if (!line) continue
    const at = line.indexOf(':')
    const name = at > 0 ? line.slice(0, at).trim() : ''
    if (!name) return { headers, error: `Ligne ${i + 1} : « Nom: valeur » attendu` }
    headers.push({ name, value: line.slice(at + 1).trim() })
  }
  return { headers, error: null }
}
