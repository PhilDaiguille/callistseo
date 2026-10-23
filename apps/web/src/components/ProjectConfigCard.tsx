import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { QueryError } from '@/components/QueryError'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { api, type ProjectConfig } from '@/lib/api'
import { toConfig, toForm, type ConfigForm } from '@/lib/config'

const AREA = 'field min-h-24 w-full py-2 font-mono text-xs'

function Form({ projectId, saved }: { projectId: number; saved: ProjectConfig }) {
  const queryClient = useQueryClient()
  const [form, setForm] = useState<ConfigForm>(() => toForm(saved))
  const set = (key: keyof ConfigForm) => (e: { target: { value: string } }) => setForm((f) => ({ ...f, [key]: e.target.value }))
  const save = useMutation({
    mutationFn: async () => api.saveProjectConfig(projectId, toConfig(form, saved)),
    onSuccess: (cfg) => {
      queryClient.setQueryData(['project-config', projectId], cfg)
      setForm(toForm(cfg))
    },
  })

  return (
    <form
      className="grid gap-4"
      onSubmit={(e) => {
        e.preventDefault()
        save.mutate()
      }}
    >
      <div className="grid gap-4 md:grid-cols-2">
        <div className="grid gap-1.5">
          <Label htmlFor="cfg-include">Suivre seulement (un motif par ligne)</Label>
          <textarea id="cfg-include" className={AREA} value={form.include} onChange={set('include')} placeholder="/blog/" spellCheck={false} />
          <p className="text-xs text-muted-foreground">Expressions régulières cherchées dans l’URL. Vide = tout le site.</p>
        </div>
        <div className="grid gap-1.5">
          <Label htmlFor="cfg-exclude">Ne jamais suivre</Label>
          <textarea id="cfg-exclude" className={AREA} value={form.exclude} onChange={set('exclude')} placeholder={'\\.pdf$\n/panier'} spellCheck={false} />
          <p className="text-xs text-muted-foreground">L’exclusion l’emporte sur l’inclusion. L’URL de départ n’est jamais filtrée.</p>
        </div>
        <div className="grid gap-1.5">
          <Label htmlFor="cfg-strip">Paramètres d’URL à ignorer</Label>
          <textarea id="cfg-strip" className={AREA} value={form.strip} onChange={set('strip')} placeholder={'utm_*\nsessionid'} spellCheck={false} />
          <p className="text-xs text-muted-foreground">Retirés avant déduplication ; <code>utm_*</code> = tous ceux qui commencent par <code>utm_</code>.</p>
        </div>
        <div className="grid gap-1.5">
          <Label htmlFor="cfg-robots">robots.txt personnalisé</Label>
          <textarea id="cfg-robots" className={AREA} value={form.robots} onChange={set('robots')} placeholder={'User-agent: *\nDisallow: /test/'} spellCheck={false} />
          <p className="text-xs text-muted-foreground">Remplace celui du site pour cet audit (pour essayer une version à venir). Vide = celui du site.</p>
        </div>
      </div>
      <div className="grid gap-1.5">
        <Label htmlFor="cfg-extract">Extraction personnalisée et recherche dans le source</Label>
        <textarea
          id="cfg-extract"
          className={AREA}
          value={form.extractors}
          onChange={set('extractors')}
          placeholder={'ga: regex: UA-(\\d+)\nprix: css: .price\nlien: css@href: a.cta\nbrouillon: contains: TODO'}
          spellCheck={false}
        />
        <p className="text-xs text-muted-foreground">
          Une ligne par extracteur : <code>nom: type: motif</code>. <code>regex</code> = premier groupe capturant du HTML source ; <code>css</code> = texte des éléments
          (<code>css@href</code> = valeur de l’attribut) ; <code>contains</code> = nombre d’occurrences du texte. 10 au plus ; résultats dans la colonne « Extraction ».
        </p>
      </div>
      <div className="flex flex-wrap items-end gap-4">
        {(
          [
            ['maxPages', 'Pages maximum', 'cfg-pages'],
            ['maxDepth', 'Profondeur maximale', 'cfg-depth'],
            ['delayMs', 'Délai (ms)', 'cfg-delay'],
            ['maxUrlLength', 'Longueur d’URL maximale', 'cfg-len'],
          ] as const
        ).map(([key, label, id]) => (
          <div key={id} className="grid gap-1.5">
            <Label htmlFor={id}>{label}</Label>
            <Input id={id} type="number" min={0} value={form[key]} onChange={set(key)} placeholder="défaut" className="w-40" />
          </div>
        ))}
        <Button type="submit" disabled={save.isPending}>
          {save.isPending ? 'Enregistrement…' : 'Enregistrer la configuration'}
        </Button>
        {save.isSuccess && <span className="text-sm text-muted-foreground" role="status">Enregistré.</span>}
      </div>
      <QueryError error={save.error} />
    </form>
  )
}

/** Configuration d'audit du projet : périmètre des URL, paramètres ignorés, robots.txt d'essai, limites. */
export function ProjectConfigCard({ projectId }: { projectId: number }) {
  const config = useQuery({ queryKey: ['project-config', projectId], queryFn: () => api.projectConfig(projectId) })
  return (
    <Card>
      <CardHeader>
        <CardTitle>Configuration de l’audit</CardTitle>
        <CardDescription>
          Appliquée à chaque audit de ce projet. Les plafonds du serveur s’appliquent toujours par-dessus.
        </CardDescription>
      </CardHeader>
      <CardContent>
        {config.error ? <QueryError error={config.error} /> : config.data ? <Form projectId={projectId} saved={config.data} /> : <p className="text-sm text-muted-foreground">Chargement…</p>}
      </CardContent>
    </Card>
  )
}
