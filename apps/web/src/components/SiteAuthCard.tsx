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
  const parts = [s.basic && 'authentification Basic', s.headers > 0 && `${s.headers} en-tête(s)`, s.cookies && 'cookies', s.form && 'connexion par formulaire'].filter(
    Boolean,
  )
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
  const [form, setForm] = useState({ url: '', username: '', password: '', usernameField: '', passwordField: '' })
  const setFormField = (k: keyof typeof form) => (e: React.ChangeEvent<HTMLInputElement>) => setForm({ ...form, [k]: e.target.value })
  const [formError, setFormError] = useState<string | null>(null)
  const reset = () => {
    setUser('')
    setPassword('')
    setHeaders('')
    setCookies('')
    setForm({ url: '', username: '', password: '', usernameField: '', passwordField: '' })
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
          jamais aux liens externes ; ils sont chiffrés sur le serveur et jamais réaffichés. Connexion par formulaire possible (ci-dessous), ou cookie de
          session copié depuis votre navigateur.
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
              const anyForm = form.url || form.username || form.password
              if (anyForm && !(form.url && form.username && form.password))
                return setFormError('Connexion par formulaire : adresse de la page, identifiant et mot de passe sont requis.')
              setFormError(null)
              save.mutate({
                basic: user ? { user, password } : undefined,
                headers: parsed.headers,
                cookies: cookies.trim() || undefined,
                form: anyForm
                  ? {
                      url: form.url.trim(),
                      username: form.username,
                      password: form.password,
                      username_field: form.usernameField.trim() || undefined,
                      password_field: form.passwordField.trim() || undefined,
                    }
                  : undefined,
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
            <fieldset className="grid gap-3 border-t border-line pt-3">
              <legend className="text-sm font-semibold">Connexion par formulaire</legend>
              <p className="text-xs text-muted-foreground">
                Avant l’audit, la page de connexion est chargée, le formulaire rempli et envoyé (jeton CSRF compris) ; les cookies de session servent ensuite
                pour tout l’audit. Les liens de déconnexion ne sont jamais suivis. La page doit être sur le site du projet.
              </p>
              <div className="grid gap-1.5">
                <Label htmlFor="auth-form-url">Page de connexion</Label>
                <Input id="auth-form-url" type="url" value={form.url} onChange={setFormField('url')} placeholder="https://preprod.exemple.fr/connexion" spellCheck={false} />
              </div>
              <div className="grid gap-3 sm:grid-cols-2">
                <div className="grid gap-1.5">
                  <Label htmlFor="auth-form-user">Identifiant</Label>
                  <Input id="auth-form-user" value={form.username} onChange={setFormField('username')} autoComplete="off" spellCheck={false} />
                </div>
                <div className="grid gap-1.5">
                  <Label htmlFor="auth-form-password">Mot de passe</Label>
                  <Input id="auth-form-password" type="password" value={form.password} onChange={setFormField('password')} autoComplete="new-password" />
                </div>
                <div className="grid gap-1.5">
                  <Label htmlFor="auth-form-ufield">Nom du champ identifiant (facultatif)</Label>
                  <Input id="auth-form-ufield" value={form.usernameField} onChange={setFormField('usernameField')} placeholder="détection automatique" spellCheck={false} />
                </div>
                <div className="grid gap-1.5">
                  <Label htmlFor="auth-form-pfield">Nom du champ mot de passe (facultatif)</Label>
                  <Input id="auth-form-pfield" value={form.passwordField} onChange={setFormField('passwordField')} placeholder="détection automatique" spellCheck={false} />
                </div>
              </div>
            </fieldset>
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
