import { beforeEach, describe, expect, it, vi } from 'vitest'

const { check } = vi.hoisted(() => ({ check: vi.fn() }))
vi.mock('@tauri-apps/plugin-updater', () => ({ check }))

// `isDesktop` est évalué à l'import : on réimporte le module après avoir simulé l'environnement.
async function loadUpdater(desktop: boolean) {
  vi.resetModules()
  vi.stubGlobal('window', desktop ? { __TAURI_INTERNALS__: {} } : {})
  return import('./updater')
}

describe('checkForUpdate', () => {
  beforeEach(() => {
    check.mockReset()
    vi.unstubAllGlobals()
  })

  it('ne fait rien hors desktop', async () => {
    const { checkForUpdate } = await loadUpdater(false)
    expect(await checkForUpdate()).toBeNull()
    expect(check).not.toHaveBeenCalled()
  })

  it('renvoie la mise à jour trouvée en desktop', async () => {
    const update = { version: '0.1.1' }
    check.mockResolvedValue(update)
    const { checkForUpdate } = await loadUpdater(true)
    expect(await checkForUpdate()).toBe(update)
  })

  it('reste silencieux si la vérification échoue (hors ligne)', async () => {
    check.mockRejectedValue(new Error('network'))
    const { checkForUpdate } = await loadUpdater(true)
    expect(await checkForUpdate()).toBeNull()
  })
})
