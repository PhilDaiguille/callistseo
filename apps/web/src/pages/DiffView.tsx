import type { ReactNode } from 'react'
import { useQuery } from '@tanstack/react-query'
import { SeverityBadge } from '@/components/Badges'
import { Stat } from '@/components/Stat'
import { EmptyState } from '@/components/EmptyState'
import { QueryError } from '@/components/QueryError'
import { UrlLink } from '@/components/UrlLink'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { api, type DiffResult, type IssueItem } from '@/lib/api'
import { formatDate } from '@/lib/format'
import { FIELD_LABEL, ruleInfo } from '@/lib/rules'
import { useQueryParams } from '@/lib/useQueryParams'

function Section({ title, count, children }: { title: string; count: number; children: ReactNode }) {
  if (count === 0) return null
  return (
    <section className="grid gap-2">
      <h3 className="text-lg font-bold">
        {title} <span className="text-muted-foreground tabular-nums">({count})</span>
      </h3>
      {children}
    </section>
  )
}

function IssueTable({ items }: { items: IssueItem[] }) {
  return (
    <div className="table-panel">
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead className="w-32">Gravité</TableHead>
            <TableHead className="w-48">Règle</TableHead>
            <TableHead>URL</TableHead>
            <TableHead>Détail</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {items.map((i, n) => (
            <TableRow key={`${i.rule}|${i.url}|${n}`}>
              <TableCell>
                <SeverityBadge severity={i.severity} />
              </TableCell>
              <TableCell>{ruleInfo(i.rule).label}</TableCell>
              <TableCell className="max-w-md">
                <UrlLink url={i.url} />
              </TableCell>
              <TableCell className="max-w-sm truncate" title={i.message}>
                {i.message}
              </TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>
    </div>
  )
}

function UrlList({ urls }: { urls: string[] }) {
  return (
    <ul className="max-h-64 overflow-auto panel px-3 py-2">
      {urls.map((u) => (
        <li key={u} className="py-0.5">
          <UrlLink url={u} />
        </li>
      ))}
    </ul>
  )
}

function Result({ d }: { d: DiffResult }) {
  const c = d.counts
  const empty = Object.entries(c).every(([k, v]) => k === 'new_errors' || v === 0)
  const shown = (v: string) => (v === '' ? '∅' : v)
  return (
    <div className="grid gap-5">
      <p className="text-sm text-muted-foreground">
        Audit #{d.before.id} ({formatDate(d.before.started_at)}) → audit #{d.after.id} ({formatDate(d.after.started_at)})
      </p>
      <div className="flex flex-wrap gap-3">
        <Stat label="Nouvelles erreurs" value={c.new_errors} alert />
        <Stat label="Nouveaux problèmes" value={c.new_issues} />
        <Stat label="Problèmes résolus" value={c.resolved_issues} />
        <Stat label="Pages ajoutées" value={c.added_pages} />
        <Stat label="Pages supprimées" value={c.removed_pages} />
        <Stat label="Pages modifiées" value={c.changed_pages} />
      </div>
      {empty && <EmptyState mood="happy">Aucune différence entre ces deux audits.</EmptyState>}
      {d.truncated && <p className="text-sm text-muted-foreground">Les listes sont limitées aux premiers éléments ; les compteurs restent exacts.</p>}

      <Section title="Nouveaux problèmes" count={c.new_issues}>
        <IssueTable items={d.new_issues} />
      </Section>
      <Section title="Problèmes résolus" count={c.resolved_issues}>
        <IssueTable items={d.resolved_issues} />
      </Section>
      <Section title="Pages modifiées" count={c.changed_pages}>
        <div className="table-panel">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>URL</TableHead>
                <TableHead>Changements</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {d.changed_pages.map((p) => (
                <TableRow key={p.url}>
                  <TableCell className="max-w-sm align-top">
                    <UrlLink url={p.url} />
                  </TableCell>
                  <TableCell className="whitespace-normal">
                    <ul className="grid gap-1 text-sm">
                      {p.changes.map((ch) => (
                        <li key={ch.field}>
                          <span className="text-muted-foreground">{FIELD_LABEL[ch.field] ?? ch.field} :</span>{' '}
                          <span className="break-all">{shown(ch.before)}</span> → <span className="break-all">{shown(ch.after)}</span>
                        </li>
                      ))}
                    </ul>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </div>
      </Section>
      <Section title="Pages ajoutées" count={c.added_pages}>
        <UrlList urls={d.added_pages} />
      </Section>
      <Section title="Pages supprimées" count={c.removed_pages}>
        <UrlList urls={d.removed_pages} />
      </Section>
    </div>
  )
}

/** Compare l'audit courant à un audit terminé du même projet (par défaut le précédent). */
export function DiffView({ crawlId, projectId }: { crawlId: number; projectId: number }) {
  const [params, update] = useQueryParams()
  const project = useQuery({ queryKey: ['project', projectId], queryFn: () => api.project(projectId) })

  const candidates = (project.data?.crawls ?? []).filter((c) => c.status === 'done' && c.id !== crawlId).sort((a, b) => b.id - a.id)
  const previous = candidates.find((c) => c.id < crawlId) ?? candidates[0]
  const baseParam = Number(params.get('base'))
  const base = candidates.some((c) => c.id === baseParam) ? baseParam : previous?.id

  const diff = useQuery({
    queryKey: ['diff', base, crawlId],
    queryFn: () => api.diff(base!, crawlId),
    enabled: base !== undefined,
  })

  if (project.error) return <QueryError error={project.error} />
  if (!project.data) return <p className="text-sm text-muted-foreground">Chargement…</p>
  if (base === undefined) {
    return (
      <EmptyState mood="sleepy">Aucun autre audit terminé pour ce projet : lancez-en un second pour comparer.</EmptyState>
    )
  }

  return (
    <div className="grid gap-4">
      <label className="flex items-center gap-2 text-sm">
        Comparer à l'audit
        <select
          value={base}
          onChange={(e) => update({ base: e.target.value })}
          className="field"
        >
          {candidates.map((c) => (
            <option key={c.id} value={c.id}>
              #{c.id} — {formatDate(c.started_at)} ({c.pages} URL)
            </option>
          ))}
        </select>
      </label>
      <QueryError error={diff.error} />
      {diff.isPending && !diff.error && <p className="text-sm text-muted-foreground">Comparaison…</p>}
      {diff.data && <Result d={diff.data} />}
    </div>
  )
}
