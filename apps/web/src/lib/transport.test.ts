import { beforeEach, describe, expect, it, vi } from 'vitest'

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }))
vi.mock('@tauri-apps/api/core', () => ({ invoke }))

// `isDesktop` est évalué à l'import : on réimporte le module après avoir simulé l'environnement.
async function loadTransport(desktop: boolean) {
  vi.resetModules()
  vi.stubGlobal('window', desktop ? { __TAURI_INTERNALS__: {} } : {})
  return import('./transport')
}

describe('transport', () => {
  beforeEach(() => {
    invoke.mockReset()
    vi.unstubAllGlobals()
  })

  it('desktop : passe par la commande Tauri et reconstruit une Response', async () => {
    invoke.mockResolvedValue({ status: 404, content_type: 'application/json', content_disposition: null, body: '{"error":"x"}' })
    const { apiFetch, isDesktop } = await loadTransport(true)
    expect(isDesktop).toBe(true)

    const res = await apiFetch('/api/crawls/9', { method: 'POST', body: '{}' })
    expect(invoke).toHaveBeenCalledWith('api_request', { method: 'POST', path: '/api/crawls/9', body: '{}' })
    expect([res.ok, res.status, res.headers.get('content-type')]).toEqual([false, 404, 'application/json'])
    expect(await res.json()).toEqual({ error: 'x' })

    await apiFetch('/api/projects')
    expect(invoke).toHaveBeenLastCalledWith('api_request', { method: 'GET', path: '/api/projects', body: null })
  })

  it('desktop : un 204 (suppression) ne lève pas d\u2019erreur', async () => {
    invoke.mockResolvedValue({ status: 204, content_type: null, content_disposition: null, body: '' })
    const { apiFetch } = await loadTransport(true)
    const res = await apiFetch('/api/projects/1/schedule', { method: 'DELETE' })
    expect([res.ok, res.status]).toEqual([true, 204])
    expect(invoke).toHaveBeenCalledWith('api_request', { method: 'DELETE', path: '/api/projects/1/schedule', body: null })
  })

  it('desktop : les exports passent par la boîte de dialogue native', async () => {
    invoke.mockResolvedValue(true)
    const { saveDownload } = await loadTransport(true)
    await saveDownload('crawl-1-pages.csv', new Response('url,depth\n'))
    expect(invoke).toHaveBeenCalledWith('save_file', { filename: 'crawl-1-pages.csv', content: 'url,depth\n' })
  })

  it('navigateur : utilise fetch et jamais Tauri', async () => {
    const fetchMock = vi.fn(async () => new Response('{"ok":true}'))
    vi.stubGlobal('fetch', fetchMock)
    const { apiFetch, isDesktop } = await loadTransport(false)
    expect(isDesktop).toBe(false)
    const res = await apiFetch('/api/projects')
    expect(await res.json()).toEqual({ ok: true })
    expect(fetchMock).toHaveBeenCalledOnce()
    expect(invoke).not.toHaveBeenCalled()
  })
})
