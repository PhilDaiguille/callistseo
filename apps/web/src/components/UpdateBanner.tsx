import { useEffect, useState } from 'react'
import { relaunch } from '@tauri-apps/plugin-process'
import type { Update } from '@tauri-apps/plugin-updater'
import { Button } from '@/components/ui/button'
import { checkForUpdate } from '@/lib/updater'

/** Propose une nouvelle version trouvée au démarrage ; rien n'est installé sans clic. */
export function UpdateBanner() {
  const [update, setUpdate] = useState<Update | null>(null)
  const [busy, setBusy] = useState(false)
  const [failed, setFailed] = useState(false)

  useEffect(() => {
    checkForUpdate().then(setUpdate)
  }, [])

  if (!update) return null

  const install = async () => {
    setBusy(true)
    setFailed(false)
    try {
      await update.downloadAndInstall()
      await relaunch()
    } catch {
      setFailed(true)
      setBusy(false)
    }
  }

  return (
    <div role="status" className="flex flex-wrap items-center gap-3 border-b border-line bg-accent px-4 py-2 text-sm">
      <span>
        Version {update.version} disponible{failed ? ' : l’installation a échoué, réessayez.' : '.'}
      </span>
      <Button size="sm" disabled={busy} onClick={install}>
        {busy ? 'Installation…' : 'Installer et redémarrer'}
      </Button>
    </div>
  )
}
