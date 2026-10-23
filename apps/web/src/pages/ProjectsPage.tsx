import { useState } from 'react'
import { Link } from 'react-router-dom'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { RunBadge } from '@/components/Badges'
import { EmptyState } from '@/components/EmptyState'
import { PageSkeleton } from '@/components/Skeleton'
import { QueryError } from '@/components/QueryError'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { api } from '@/lib/api'
import { formatDate } from '@/lib/format'
import { useDocumentTitle } from '@/lib/title'

function NewProjectForm({ onDone }: { onDone: () => void }) {
  const queryClient = useQueryClient()
  const [name, setName] = useState('')
  const [url, setUrl] = useState('')
  const create = useMutation({
    mutationFn: api.createProject,
    onSuccess: () => {
      setName('')
      setUrl('')
      onDone()
      void queryClient.invalidateQueries({ queryKey: ['projects'] })
    },
  })

  return (
    <Card>
      <CardHeader>
        <CardTitle>Nouveau projet</CardTitle>
        <CardDescription>Un projet correspond à un site à auditer régulièrement.</CardDescription>
      </CardHeader>
      <CardContent>
        <form
          className="grid gap-4 sm:grid-cols-[1fr_2fr_auto] sm:items-end"
          onSubmit={(e) => {
            e.preventDefault()
            create.mutate({ name: name.trim(), start_url: url.trim() })
          }}
        >
          <div className="grid gap-1.5">
            <Label htmlFor="project-name">Nom</Label>
            <Input id="project-name" autoFocus required maxLength={100} value={name} onChange={(e) => setName(e.target.value)} placeholder="Mon site" />
          </div>
          <div className="grid gap-1.5">
            <Label htmlFor="project-url">URL de départ</Label>
            <Input id="project-url" required type="url" value={url} onChange={(e) => setUrl(e.target.value)} placeholder="https://exemple.fr/" />
          </div>
          <Button type="submit" disabled={create.isPending}>
            Créer
          </Button>
        </form>
        {create.error && (
          <div className="mt-3">
            <QueryError error={create.error} />
          </div>
        )}
      </CardContent>
    </Card>
  )
}

export function ProjectsPage() {
  useDocumentTitle('Projets')
  const projects = useQuery({ queryKey: ['projects'], queryFn: api.projects })
  const [creating, setCreating] = useState(false)
  const empty = projects.data?.length === 0

  return (
    <div className="grid gap-6">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h1 className="text-3xl font-extrabold tracking-tight">Projets</h1>
        {!empty && !creating && <Button onClick={() => setCreating(true)}>Nouveau projet</Button>}
      </div>
      {(creating || empty) && <NewProjectForm onDone={() => setCreating(false)} />}
      <QueryError error={projects.error} />
      {projects.isPending && !projects.error && <PageSkeleton rows={2} />}
      {empty && !creating && (
        <EmptyState mood="sleepy">Aucun projet pour l'instant. Créez-en un ci-dessus pour lancer un premier audit.</EmptyState>
      )}
      <ul className="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
        {projects.data?.map((p) => {
          const last = p.last_crawl
          return (
            <li key={p.id}>
              <Link to={`/projects/${p.id}`} className="block h-full rounded-2xl">
                <Card variant="pop" className="h-full transition-transform hover:-translate-y-0.5 hover:bg-accent/40 motion-reduce:transition-none">
                  <CardHeader>
                    <CardTitle>{p.name}</CardTitle>
                    <CardDescription className="break-all">{p.start_url}</CardDescription>
                  </CardHeader>
                  <CardContent className="grid gap-2 text-sm">
                    {last ? (
                      <>
                        <div className="flex flex-wrap items-center gap-2">
                          <RunBadge status={last.status} />
                          {last.status === 'done' && (
                            <span className="text-muted-foreground tabular-nums">
                              {last.pages.toLocaleString('fr-FR')} URL · <b className={last.errors > 0 ? 'text-destructive' : 'text-foreground'}>{last.errors}</b> erreurs ·{' '}
                              {last.warnings.toLocaleString('fr-FR')} avertissements
                            </span>
                          )}
                        </div>
                        <span className="text-xs text-muted-foreground">Dernier audit : {formatDate(last.started_at)}</span>
                      </>
                    ) : (
                      <span className="text-xs text-muted-foreground">Aucun audit — créé le {formatDate(p.created_at)}</span>
                    )}
                  </CardContent>
                </Card>
              </Link>
            </li>
          )
        })}
      </ul>
    </div>
  )
}
