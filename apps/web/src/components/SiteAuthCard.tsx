import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { QueryError } from '@/components/QueryError'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { api, type SiteAuthBody, type SiteAuthState } from '@/lib/api'
import { parseHeaders } from '@/lib/siteAuth'

const AREA = 'field min-h-20 w-full py-2 font-mono text-xs'

function summary(s: SiteAuthState): string {
  if (!s.configured) return 'Aucun identifiant : le site est audité comme un visiteur anonyme.'
  if (!s.readable) return 'Identifiants enregistrés mais illisibles (la clé du serveur a changé) : enregistrez-les à nouveau ou retirez-les. Les audits sont bloqués en attendant.'
  const parts = [s.basic && 'authentification Basic', s.headers > 0 && `${s.headers} en-tête(s)`, s.cookies && 'cookies'].filter(Boolean)
  return `Identifiants enregistrés (${parts.join(', ')}). Ils ne sont jamais réaffichés : pour les changer, saisissez-les de nouveau.`
}

/**
 * Accès à un site protégé (préproduction, espace membre). Écriture seule : le serveur chiffre les
 * identifiants et ne renvoie que leur nature. Le formulaire remplace l'ensemble à chaque envoi.
 */
export function SiteAuthCard({ projectId }: { projectId: number }) {
  const queryClient = useQueryClient()
  const key = ['site-auth', projectId]
  const state = useQuery({ queryKey: key, queryFn: () => api.siteAuth(projectId) })
  const [user, setUser] = useState('')
  const [password, setPassword] = useState('')
  const [headers, setHeaders] = useState('')
  const [cookies, setCookies] = useState('')
  const [formError, setFormError] = useState<string | null>(null)
  const reset = () => {
    setUser('')
    setPassword('')
    setHeaders('')
    setCookies('')
    void queryClient.invalidateQueries({ queryKey: key })
  }
  const save = useMutation({ mutationFn: (body: SiteAuthBody) => api.saveSiteAuth(projectId, body), onSuccess: reset })
  const remove = useMutation({ mutationFn: () => api.deleteSiteAuth(projectId), onSuccess: reset })
  const s = state.data

  return (
    <Card>
      <CardHeader>
        <CardTitle>Site protégé</CardTitle>
        <CardDescription>
          Pour auditer une préproduction ou un espace réservé. Les identifiants ne sont envoyés qu’au site du projet (même adresse, même protocole),
          jamais aux liens externes ; ils sont chiffrés sur le serveur et jamais réaffichés. Pour une connexion par formulaire, copiez le cookie de
          session depuis votre navigateur.
        </CardDescription>
      </CardHeader>
      <CardContent className="grid gap-4">
        {s && !s.available && (
          <p className="text-sm text-muted-foreground" role="status">
            Fonction désactivée sur ce serveur : l’administrateur doit définir <code>CALLISTSEO_SECRET_KEY</code> (voir la documentation d’installation).
          </p>
        )}
        {s && (
          <p className="text-sm" role="status">
            {summary(s)}
          </p>
        )}
        {s?.available && (
          <form
            className="grid gap-3"
            autoComplete="off"
            onSubmit={(e) => {
              e.preventDefault()
              const parsed = parseHeaders(headers)
              if (parsed.error) return setFormError(parsed.error)
              if (!user && password) return setFormError('Indiquez l’utilisateur de l’authentification Basic.')
              setFormError(null)
              save.mutate({
                basic: user ? { user, password } : undefined,
                headers: parsed.headers,
                cookies: cookies.trim() || undefined,
              })
            }}
          >
            <div className="grid gap-3 sm:grid-cols-2">
              <div className="grid gap-1.5">
                <Label htmlFor="auth-user">Utilisateur (Basic)</Label>
                <Input id="auth-user" value={user} onChange={(e) => setUser(e.target.value)} autoComplete="off" spellCheck={false} />
              </div>
              <div className="grid gap-1.5">
                <Label htmlFor="auth-password">Mot de passe (Basic)</Label>
                <Input id="auth-password" type="password" value={password} onChange={(e) => setPassword(e.target.value)} autoComplete="new-password" />
              </div>
            </div>
            <div className="grid gap-1.5">
              <Label htmlFor="auth-headers">En-têtes, un par ligne</Label>
              <textarea
                id="auth-headers"
                className={AREA}
                value={headers}
                onChange={(e) => setHeaders(e.target.value)}
                placeholder={'X-Preprod-Key: votre-cle\nAuthorization: Bearer …'}
                spellCheck={false}
              />
            </div>
            <div className="grid gap-1.5">
              <Label htmlFor="auth-cookies">Cookies</Label>
              <Input id="auth-cookies" value={cookies} onChange={(e) => setCookies(e.target.value)} placeholder="session=…; autre=…" spellCheck={false} />
            </div>
            <div className="flex flex-wrap gap-2">
              <Button type="submit" disabled={save.isPending}>
                {s.configured ? 'Remplacer les identifiants' : 'Enregistrer'}
              </Button>
              {s.configured && (
                <Button type="button" variant="ghost" disabled={remove.isPending} onClick={() => remove.mutate()}>
                  Retirer
                </Button>
              )}
            </div>
          </form>
        )}
        {s && !s.available && s.configured && (
          <Button type="button" variant="ghost" className="justify-self-start" disabled={remove.isPending} onClick={() => remove.mutate()}>
            Retirer les identifiants enregistrés
          </Button>
        )}
        {formError && (
          <p className="text-sm text-destructive" role="alert">
            {formError}
          </p>
        )}
        <QueryError error={state.error ?? save.error ?? remove.error} />
      </CardContent>
    </Card>
  )
}
