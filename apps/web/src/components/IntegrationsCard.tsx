import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { QueryError } from '@/components/QueryError'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { api, type IntegrationKeysState, type IntegrationsConfig } from '@/lib/api'

const AREA = 'field min-h-24 w-full py-2 font-mono text-xs'
const SELECT = 'field h-9 w-full px-2 text-sm'

/** Ce qui sort du serveur, affiché avant d'activer une intégration (consentement explicite). */
const CONSENT = {
  psi: 'PageSpeed Insights : l’adresse des pages mesurées part chez Google.',
  gsc: 'Search Console : la propriété demandée et un jeton signé par votre compte de service partent chez Google.',
  ga4: 'GA4 : l’identifiant de propriété et un jeton signé par votre compte de service partent chez Google.',
}

interface Form {
  psi: boolean
  strategy: 'mobile' | 'desktop'
  maxUrls: string
  gsc: boolean
  property: string
  ga4: boolean
  propertyId: string
  consent: boolean
}

function toForm(i: IntegrationsConfig | undefined): Form {
  return {
    psi: !!i?.psi,
    strategy: i?.psi?.strategy ?? 'mobile',
    maxUrls: String(i?.psi?.max_urls ?? 20),
    gsc: !!i?.gsc,
    property: i?.gsc?.property ?? '',
    ga4: !!i?.ga4,
    propertyId: i?.ga4?.property_id ?? '',
    consent: false,
  }
}

function toIntegrations(f: Form): IntegrationsConfig {
  return {
    psi: f.psi ? { strategy: f.strategy, max_urls: Number(f.maxUrls) } : undefined,
    gsc: f.gsc ? { property: f.property.trim() } : undefined,
    ga4: f.ga4 ? { property_id: f.propertyId.trim() } : undefined,
  }
}

function keysSummary(k: IntegrationKeysState): string {
  if (!k.available) return 'Fonction désactivée sur ce serveur : l’administrateur doit définir CALLISTSEO_SECRET_KEY.'
  if (!k.readable) return 'Clés enregistrées mais illisibles (la clé du serveur a changé) : saisissez-les à nouveau.'
  const parts = [k.psi_key && 'clé PageSpeed', k.service_account && `compte de service ${k.service_account}`].filter(Boolean)
  return parts.length ? `Enregistré : ${parts.join(', ')}. Jamais réaffiché.` : 'Aucune clé enregistrée.'
}

/**
 * Intégrations externes (PageSpeed, Search Console, GA4) : désactivées par défaut. L’activation
 * exige d’accepter ce qui part chez Google ; les clés sont écrites seules, jamais réaffichées.
 */
export function IntegrationsCard({ projectId }: { projectId: number }) {
  const queryClient = useQueryClient()
  const configKey = ['project-config', projectId]
  const config = useQuery({ queryKey: configKey, queryFn: () => api.projectConfig(projectId) })
  const keysKey = ['integration-keys', projectId]
  const keys = useQuery({ queryKey: keysKey, queryFn: () => api.integrationKeys(projectId) })
  const [form, setForm] = useState<Form | null>(null)
  const current = form ?? toForm(config.data?.integrations)
  const set = <K extends keyof Form>(k: K, v: Form[K]) => setForm({ ...current, [k]: v })
  const [psiKey, setPsiKey] = useState('')
  const [serviceAccount, setServiceAccount] = useState('')
  const [formError, setFormError] = useState<string | null>(null)

  const save = useMutation({
    mutationFn: () => {
      if (!config.data) throw new Error('Configuration non chargée.')
      return api.saveProjectConfig(projectId, { ...config.data, integrations: toIntegrations(current) })
    },
    onSuccess: (cfg) => {
      queryClient.setQueryData(configKey, cfg)
      setForm(null)
      setFormError(null)
    },
  })
  const saveKeys = useMutation({
    mutationFn: (body: { psi_key?: string; service_account?: string }) => api.saveIntegrationKeys(projectId, body),
    onSuccess: () => {
      setPsiKey('')
      setServiceAccount('')
      void queryClient.invalidateQueries({ queryKey: keysKey })
    },
  })

  const enabled = [current.psi && 'psi', current.gsc && 'gsc', current.ga4 && 'ga4'].filter(Boolean) as (keyof typeof CONSENT)[]
  const k = keys.data

  return (
    <Card>
      <CardHeader>
        <CardTitle>Intégrations externes</CardTitle>
        <CardDescription>
          Désactivées par défaut. Les données du crawl ne quittent jamais le serveur ; seules les mesures listées ci-dessous sont demandées à Google, après
          l’audit. Une panne chez Google n’empêche pas d’avoir les résultats du crawl.
        </CardDescription>
      </CardHeader>
      <CardContent className="grid gap-5">
        <form
          className="grid gap-4"
          onSubmit={(e) => {
            e.preventDefault()
            if (enabled.length && !current.consent) return setFormError('Acceptez ce qui part chez Google avant d’enregistrer.')
            if (current.psi && !(Number(current.maxUrls) >= 1)) return setFormError('PageSpeed : indiquez un nombre de pages par audit.')
            setFormError(null)
            save.mutate()
          }}
        >
          <fieldset className="grid gap-3">
            <legend className="text-sm font-semibold">PageSpeed Insights (Core Web Vitals)</legend>
            <label className="flex items-center gap-2 text-sm">
              <input type="checkbox" checked={current.psi} onChange={(e) => set('psi', e.target.checked)} />
              Mesurer les pages de l’audit
            </label>
            {current.psi && (
              <div className="grid gap-3 sm:grid-cols-2">
                <div className="grid gap-1.5">
                  <Label htmlFor="int-psi-strategy">Stratégie</Label>
                  <select id="int-psi-strategy" className={SELECT} value={current.strategy} onChange={(e) => set('strategy', e.target.value as Form['strategy'])}>
                    <option value="mobile">Mobile (index Google)</option>
                    <option value="desktop">Bureau</option>
                  </select>
                </div>
                <div className="grid gap-1.5">
                  <Label htmlFor="int-psi-max">Pages mesurées par audit (max 100)</Label>
                  <Input id="int-psi-max" type="number" min={1} max={100} value={current.maxUrls} onChange={(e) => set('maxUrls', e.target.value)} />
                </div>
              </div>
            )}
          </fieldset>

          <fieldset className="grid gap-3 border-t border-line pt-3">
            <legend className="text-sm font-semibold">Search Console</legend>
            <label className="flex items-center gap-2 text-sm">
              <input type="checkbox" checked={current.gsc} onChange={(e) => set('gsc', e.target.checked)} />
              Ajouter les impressions et clics (28 jours)
            </label>
            {current.gsc && (
              <div className="grid gap-1.5">
                <Label htmlFor="int-gsc-property">Propriété</Label>
                <Input
                  id="int-gsc-property"
                  value={current.property}
                  onChange={(e) => set('property', e.target.value)}
                  placeholder="sc-domain:exemple.fr ou https://www.exemple.fr/"
                  spellCheck={false}
                />
              </div>
            )}
          </fieldset>

          <fieldset className="grid gap-3 border-t border-line pt-3">
            <legend className="text-sm font-semibold">Google Analytics 4</legend>
            <label className="flex items-center gap-2 text-sm">
              <input type="checkbox" checked={current.ga4} onChange={(e) => set('ga4', e.target.checked)} />
              Ajouter les sessions et vues (28 jours)
            </label>
            {current.ga4 && (
              <div className="grid gap-1.5">
                <Label htmlFor="int-ga4-property">Identifiant numérique de la propriété</Label>
                <Input id="int-ga4-property" value={current.propertyId} onChange={(e) => set('propertyId', e.target.value)} placeholder="123456789" spellCheck={false} />
                <p className="text-xs text-muted-foreground">Admin → Détails de la propriété (pas l’identifiant G-…).</p>
              </div>
            )}
          </fieldset>

          {enabled.length > 0 && (
            <div className="grid gap-2 rounded-md border border-line p-3 text-sm">
              <p className="font-semibold">Ce qui part chez Google</p>
              <ul className="list-disc pl-5 text-muted-foreground">
                {enabled.map((n) => (
                  <li key={n}>{CONSENT[n]}</li>
                ))}
              </ul>
              <label className="flex items-center gap-2">
                <input type="checkbox" checked={current.consent} onChange={(e) => set('consent', e.target.checked)} />
                J’accepte ces envois pour ce projet
              </label>
            </div>
          )}

          <div>
            <Button type="submit" disabled={save.isPending || config.isLoading}>
              {save.isPending ? 'Enregistrement…' : 'Enregistrer les intégrations'}
            </Button>
          </div>
          {formError && (
            <p className="text-sm text-destructive" role="alert">
              {formError}
            </p>
          )}
          {save.isSuccess && !form && <span className="text-sm text-muted-foreground" role="status">Enregistré.</span>}
        </form>

        <div className="grid gap-3 border-t border-line pt-4">
          <h3 className="text-sm font-semibold">Clés</h3>
          {k && (
            <p className="text-sm" role="status">
              {keysSummary(k)}
            </p>
          )}
          {k?.available && (
            <form
              className="grid gap-3"
              autoComplete="off"
              onSubmit={(e) => {
                e.preventDefault()
                const body: { psi_key?: string; service_account?: string } = {}
                if (psiKey.trim()) body.psi_key = psiKey.trim()
                if (serviceAccount.trim()) body.service_account = serviceAccount
                if (!Object.keys(body).length) return
                saveKeys.mutate(body)
              }}
            >
              <div className="grid gap-1.5">
                <Label htmlFor="int-psi-key">Clé d’API PageSpeed (facultative, sans elle le quota est très bas)</Label>
                <Input id="int-psi-key" type="password" value={psiKey} onChange={(e) => setPsiKey(e.target.value)} autoComplete="new-password" />
              </div>
              <div className="grid gap-1.5">
                <Label htmlFor="int-sa">Compte de service Google (fichier JSON, pour Search Console et GA4)</Label>
                <textarea
                  id="int-sa"
                  className={AREA}
                  value={serviceAccount}
                  onChange={(e) => setServiceAccount(e.target.value)}
                  placeholder={'{ "type": "service_account", … }'}
                  spellCheck={false}
                />
                <p className="text-xs text-muted-foreground">
                  Ajoutez l’adresse du compte comme utilisateur (lecture seule) dans Search Console et dans GA4.
                </p>
              </div>
              <div className="flex flex-wrap gap-2">
                <Button type="submit" disabled={saveKeys.isPending}>
                  Enregistrer les clés
                </Button>
                {k.psi_key && (
                  <Button type="button" variant="ghost" disabled={saveKeys.isPending} onClick={() => saveKeys.mutate({ psi_key: '' })}>
                    Retirer la clé PageSpeed
                  </Button>
                )}
                {k.service_account && (
                  <Button type="button" variant="ghost" disabled={saveKeys.isPending} onClick={() => saveKeys.mutate({ service_account: '' })}>
                    Retirer le compte de service
                  </Button>
                )}
              </div>
            </form>
          )}
          <QueryError error={config.error ?? keys.error ?? save.error ?? saveKeys.error} />
        </div>
      </CardContent>
    </Card>
  )
}
