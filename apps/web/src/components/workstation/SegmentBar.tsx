import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { QueryError } from '@/components/QueryError'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { api, type IssueReport } from '@/lib/api'
import { describe, emptySegmentForm, hasCriterion, toDefinition, type SegmentForm } from '@/lib/segment'

/** Segments du projet : un filtre enregistré, combinable avec le rapport affiché. */
export function SegmentBar({
  crawlId,
  projectId,
  active,
  issues,
  onSelect,
}: {
  crawlId: number
  projectId: number
  active: number | null
  issues: IssueReport[]
  onSelect: (id: number | null) => void
}) {
  const queryClient = useQueryClient()
  const counts = useQuery({ queryKey: ['segments', crawlId], queryFn: () => api.segments(crawlId) })
  const defs = useQuery({ queryKey: ['segment-defs', projectId], queryFn: () => api.projectSegments(projectId) })
  const [open, setOpen] = useState(false)
  const [form, setForm] = useState<SegmentForm>(emptySegmentForm)
  const refresh = () => {
    void queryClient.invalidateQueries({ queryKey: ['segments', crawlId] })
    void queryClient.invalidateQueries({ queryKey: ['segment-defs', projectId] })
  }
  const create = useMutation({
    mutationFn: () => {
      const definition = toDefinition(form)
      if (!hasCriterion(definition)) throw new Error('Renseignez au moins un critère.')
      return api.createSegment(projectId, { name: form.name, definition })
    },
    onSuccess: (seg) => {
      refresh()
      setForm(emptySegmentForm)
      setOpen(false)
      onSelect(seg.id)
    },
  })
  const remove = useMutation({
    mutationFn: (id: number) => api.deleteSegment(id),
    onSuccess: (_, id) => {
      refresh()
      if (active === id) onSelect(null)
    },
  })
  const set = (key: keyof SegmentForm) => (e: { target: { value: string } }) => setForm((f) => ({ ...f, [key]: e.target.value }))
  const list = counts.data?.segments ?? []
  const current = defs.data?.segments.find((s) => s.id === active)

  return (
    <div className="flex flex-wrap items-center gap-2">
      <label className="text-xs font-semibold text-muted-foreground" htmlFor="segment-select">
        Segment
      </label>
      <select id="segment-select" className="field h-8 text-[0.8rem]" value={active ?? ''} onChange={(e) => onSelect(e.target.value ? Number(e.target.value) : null)} title={current ? describe(current.definition) : undefined}>
        <option value="">Toutes les URL</option>
        {list.map((s) => (
          <option key={s.id} value={s.id}>
            {s.name} ({s.count.toLocaleString('fr-FR')})
          </option>
        ))}
      </select>
      {active !== null && (
        <Button variant="ghost" size="xs" disabled={remove.isPending} onClick={() => remove.mutate(active)}>
          Supprimer ce segment
        </Button>
      )}
      <Button variant="outline" size="xs" aria-expanded={open} onClick={() => setOpen((o) => !o)}>
        {open ? 'Fermer' : 'Nouveau segment'}
      </Button>
      {open && (
        <form
          className="panel-pop mt-1 grid w-full gap-3 p-3 md:grid-cols-3"
          onSubmit={(e) => {
            e.preventDefault()
            create.mutate()
          }}
        >
          <label className="grid gap-1 text-xs font-semibold">
            Nom
            <Input value={form.name} onChange={set('name')} required maxLength={60} placeholder="Blog en erreur" />
          </label>
          <label className="grid gap-1 text-xs font-semibold">
            L’URL contient
            <Input value={form.contains} onChange={set('contains')} placeholder="/blog/" />
          </label>
          <label className="grid gap-1 text-xs font-semibold">
            L’URL commence par
            <Input value={form.prefix} onChange={set('prefix')} placeholder="https://site.fr/blog/" />
          </label>
          <label className="grid gap-1 text-xs font-semibold">
            Statut
            <select className="field" value={form.statusClass} onChange={set('statusClass')}>
              <option value="">Tous</option>
              <option value="2">2xx</option>
              <option value="3">3xx</option>
              <option value="4">4xx</option>
              <option value="5">5xx</option>
            </select>
          </label>
          <label className="grid gap-1 text-xs font-semibold">
            Profondeur maximale
            <Input type="number" min={0} value={form.maxDepth} onChange={set('maxDepth')} />
          </label>
          <label className="grid gap-1 text-xs font-semibold">
            A un problème de la règle
            <select className="field" value={form.issueRule} onChange={set('issueRule')}>
              <option value="">Aucune</option>
              {issues.map((i) => (
                <option key={i.rule} value={i.rule}>
                  {i.label}
                </option>
              ))}
            </select>
          </label>
          <label className="grid gap-1 text-xs font-semibold">
            Extraction trouvée (nom)
            <Input value={form.extracted} onChange={set('extracted')} placeholder="ga" />
          </label>
          <div className="flex items-end gap-2 md:col-span-2">
            <Button type="submit" size="sm" disabled={create.isPending}>
              Enregistrer le segment
            </Button>
            <p className="text-xs text-muted-foreground">Tous les critères renseignés doivent être vrais.</p>
          </div>
          <div className="md:col-span-3">
            <QueryError error={create.error} />
          </div>
        </form>
      )}
      <QueryError error={remove.error} />
    </div>
  )
}
