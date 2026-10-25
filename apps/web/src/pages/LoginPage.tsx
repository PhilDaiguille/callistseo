import { useState } from 'react'
import { useNavigate, useSearchParams } from 'react-router-dom'
import { useQueryClient } from '@tanstack/react-query'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Mascot } from '@/components/Mascot'
import { ApiError, api, safeNext, setToken } from '@/lib/api'

/** Connexion d'équipe : un mot de passe partagé (le jeton du serveur, `--token`). */
export function LoginPage() {
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const [params] = useSearchParams()
  const [password, setPassword] = useState('')
  const [remember, setRemember] = useState(true)
  const [error, setError] = useState('')
  const [busy, setBusy] = useState(false)

  const submit = async () => {
    setBusy(true)
    setError('')
    setToken(password.trim(), remember)
    try {
      await api.projects() // vérifie l'accès
      queryClient.clear()
      navigate(safeNext(params.get('next')), { replace: true })
    } catch (e) {
      setToken('')
      setError(e instanceof ApiError && e.status === 401 ? 'Mot de passe incorrect.' : e instanceof Error ? e.message : String(e))
    } finally {
      setBusy(false)
    }
  }

  return (
    <main className="grid min-h-svh place-items-center bg-background px-4 text-foreground">
      <form
        className="panel-pop w-full max-w-sm space-y-5 p-6"
        onSubmit={(e) => {
          e.preventDefault()
          void submit()
        }}
      >
        <div className="flex items-center gap-3">
          <Mascot size={44} mood="happy" />
          <div>
            <h1 className="font-heading text-2xl font-extrabold tracking-tight">CallistSEO</h1>
            <p className="text-sm text-muted-foreground">Accès de l’équipe</p>
          </div>
        </div>
        <div className="space-y-2">
          <Label htmlFor="password">Mot de passe de l’équipe</Label>
          <Input
            id="password"
            type="password"
            autoComplete="current-password"
            autoFocus
            required
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            aria-invalid={error ? true : undefined}
            aria-describedby={error ? 'login-error' : undefined}
          />
        </div>
        <Label className="font-normal">
          <input type="checkbox" checked={remember} onChange={(e) => setRemember(e.target.checked)} className="size-4 accent-[var(--ring)]" />
          Se souvenir de moi sur ce poste
        </Label>
        {error && (
          <p id="login-error" role="alert" className="rounded-xl border border-destructive/40 bg-destructive-soft px-3 py-2 text-sm text-destructive">
            {error}
          </p>
        )}
        <Button type="submit" className="w-full" disabled={busy}>
          {busy ? 'Connexion…' : 'Se connecter'}
        </Button>
        <p className="text-xs text-muted-foreground">Le mot de passe vous est donné par l’administrateur de cette instance.</p>
      </form>
    </main>
  )
}
