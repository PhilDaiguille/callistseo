import { isDesktop } from './transport'

/**
 * Cherche une version plus récente sur GitHub (application desktop seulement).
 * Hors desktop, hors ligne ou en cas d'erreur : `null`, sans bruit. L'application reste utilisable.
 */
export async function checkForUpdate() {
  if (!isDesktop) return null
  try {
    const { check } = await import('@tauri-apps/plugin-updater')
    return await check()
  } catch {
    return null
  }
}
