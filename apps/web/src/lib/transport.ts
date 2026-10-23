import { invoke } from '@tauri-apps/api/core'

/** Vrai dans l'application desktop (Tauri) : l'API Rust est appelée en processus, sans réseau. */
export const isDesktop = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

interface LocalReply {
  status: number
  content_type: string | null
  content_disposition: string | null
  body: string
}

/**
 * `fetch` de l'API : HTTP dans le navigateur, commande Tauri en desktop.
 * Dans les deux cas on renvoie une `Response` standard, le reste du code ne voit pas la différence.
 */
export async function apiFetch(path: string, init: { method?: string; headers?: Headers; body?: string } = {}): Promise<Response> {
  if (!isDesktop) return fetch(path, init)
  const reply = await invoke<LocalReply>('api_request', { method: init.method ?? 'GET', path, body: init.body ?? null })
  const headers = new Headers()
  if (reply.content_type) headers.set('Content-Type', reply.content_type)
  // 204/205/304 : une `Response` avec un corps (même vide) est refusée par le navigateur.
  const noBody = [204, 205, 304].includes(reply.status)
  return new Response(noBody ? null : reply.body, { status: reply.status, headers })
}

/** Enregistre un export : boîte de dialogue native en desktop, téléchargement dans le navigateur. */
export async function saveDownload(filename: string, res: Response): Promise<void> {
  if (isDesktop) {
    await invoke<boolean>('save_file', { filename, content: await res.text() })
    return
  }
  const url = URL.createObjectURL(await res.blob())
  const a = document.createElement('a')
  a.href = url
  a.download = filename
  a.click()
  URL.revokeObjectURL(url)
}

/** Ouvre une adresse web dans le navigateur du système (desktop : commande Tauri, qui revalide le schéma). */
export async function openExternal(url: string): Promise<void> {
  if (isDesktop) await invoke('open_url', { url })
  else window.open(url, '_blank', 'noopener,noreferrer')
}
