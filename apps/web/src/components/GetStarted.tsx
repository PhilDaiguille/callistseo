import { useState } from 'react'
import { Link } from 'react-router-dom'
import { useQuery } from '@tanstack/react-query'
import { CalendarClock, Check, ChevronDown, Copy, Play, SearchCheck, Sparkles, X, type LucideIcon } from 'lucide-react'
import { CopyButton } from '@/components/CopyButton'
import { Button, buttonVariants } from '@/components/ui/button'
import { api, type ProjectConfig, type Schedule } from '@/lib/api'
import { cn } from '@/lib/utils'

export interface StartStep {
  id: string
  title: string
  hint: string
  Icon: LucideIcon
  done: boolean
  action?: { to: string; label: string }
}

/** Étapes « Premiers pas », cochées d'après les données déjà chargées (aucun état à part). */
export function startSteps(
  projectId: number,
  audits: number,
  schedule: Schedule | null | undefined,
  config: ProjectConfig | undefined,
): StartStep[] {
  const base = `/projects/${projectId}`
  return [
    { id: 'audit', title: 'Lancer un premier audit', hint: 'Explorez le site et obtenez son score de santé.', Icon: Play, done: audits > 0 },
    {
      id: 'schedule',
      title: 'Planifier des audits réguliers',
      hint: 'Un audit automatique pour repérer les régressions au fil du temps.',
      Icon: CalendarClock,
      done: !!schedule?.enabled,
      action: { to: `${base}?tab=schedule`, label: 'Planifier' },
    },
    {
      id: 'gsc',
      title: 'Brancher Search Console',
      hint: 'Croisez les problèmes avec les clics et impressions de vos pages.',
      Icon: SearchCheck,
      done: !!config?.integrations?.gsc,
      action: { to: `${base}?tab=config`, label: 'Configurer' },
    },
  ]
}

/** Choix « masquer » mémorisé par navigateur ; sans stockage, il vaut pour la session. */
function useDismissed(key: string): [boolean, () => void] {
  const [dismissed, setDismissed] = useState(() => {
    try {
      return localStorage.getItem(key) === '1'
    } catch {
      return false
    }
  })
  return [
    dismissed,
    () => {
      setDismissed(true)
      try {
        localStorage.setItem(key, '1')
      } catch {
        // stockage indisponible
      }
    },
  ]
}

/** Liste « Premiers pas » d'un projet ; disparaît une fois tout fait ou masquée. */
export function GetStarted({ projectId, audits }: { projectId: number; audits: number }) {
  // Mêmes clés que ScheduleCard et ProjectConfigCard : cache partagé.
  const schedule = useQuery({ queryKey: ['schedule', projectId], queryFn: () => api.schedule(projectId) })
  const config = useQuery({ queryKey: ['project-config', projectId], queryFn: () => api.projectConfig(projectId) })
  const [dismissed, dismiss] = useDismissed(`callistseo.start.${projectId}`)
  const [showDone, setShowDone] = useState(false)
  if (dismissed || schedule.isPending || config.isPending) return null
  const steps = startSteps(projectId, audits, schedule.data, config.data)
  const todo = steps.filter((s) => !s.done)
  const done = steps.filter((s) => s.done)
  if (todo.length === 0) return null

  return (
    <section className="panel overflow-hidden" aria-labelledby="premiers-pas">
      <div className="flex items-start justify-between gap-3 p-5">
        <div>
          <h2 id="premiers-pas" className="text-xl font-bold">Premiers pas</h2>
          <p className="text-sm text-muted-foreground">Auditez le site, suivez-le dans le temps et branchez vos outils.</p>
        </div>
        <Button variant="ghost" size="icon-sm" aria-label="Masquer les premiers pas" onClick={dismiss}>
          <X aria-hidden />
        </Button>
      </div>
      <ul className="divide-y divide-line border-t border-line">
        {todo.map((s) => (
          <StepRow key={s.id} step={s} />
        ))}
        {done.length > 0 && (
          <li>
            <button
              type="button"
              aria-expanded={showDone}
              onClick={() => setShowDone(!showDone)}
              className="flex w-full items-center gap-2 px-5 py-3 text-left text-sm font-medium hover:bg-accent"
            >
              <Check className="size-4 text-success" aria-hidden />
              {done.length} terminée{done.length > 1 ? 's' : ''}
              <ChevronDown className={cn('ml-auto size-4 text-muted-foreground transition-transform', showDone && 'rotate-180')} aria-hidden />
            </button>
            {showDone && (
              <ul className="divide-y divide-line border-t border-line">
                {done.map((s) => (
                  <StepRow key={s.id} step={s} />
                ))}
              </ul>
            )}
          </li>
        )}
      </ul>
    </section>
  )
}

function StepRow({ step }: { step: StartStep }) {
  const { title, hint, Icon, done, action } = step
  return (
    <li className="flex items-center gap-4 px-5 py-3">
      <span className="grid size-9 shrink-0 place-items-center rounded-xl border border-line bg-background">
        {done ? <Check className="size-4 text-success" aria-hidden /> : <Icon className="size-4" aria-hidden />}
      </span>
      <div className="min-w-0 flex-1">
        <p className={cn('text-sm font-semibold', done && 'text-muted-foreground line-through')}>{title}</p>
        <p className="text-sm text-muted-foreground">{hint}</p>
      </div>
      {!done && action && (
        <Link to={action.to} className={cn(buttonVariants({ variant: 'outline', size: 'sm' }))}>
          {action.label}
        </Link>
      )}
      {done && <span className="sr-only">Terminée</span>}
    </li>
  )
}

/**
 * Fait connaître le serveur MCP (`callistseo mcp`) : demande d'exemple et commande à copier.
 * Aucun appel sortant : l'utilisateur colle lui-même dans son assistant.
 */
export function AssistantCard({ projectName, startUrl }: { projectName: string; startUrl: string }) {
  const [dismissed, dismiss] = useDismissed('callistseo.assistant')
  const [copied, setCopied] = useState(false)
  if (dismissed) return null
  const prompt = `Utilise CallistSEO pour auditer ${startUrl} (projet « ${projectName} ») et donne-moi les 3 corrections les plus importantes.`
  const command = 'claude mcp add callistseo -- callistseo mcp --db ~/callistseo.db'
  return (
    <section className="panel relative grid gap-3 p-5 md:grid-cols-[auto_minmax(0,1fr)_auto] md:items-center" aria-labelledby="assistant-ia">
      <span className="grid size-11 place-items-center rounded-xl border border-line bg-background">
        <Sparkles className="size-5" aria-hidden />
      </span>
      <div className="min-w-0">
        <h2 id="assistant-ia" className="text-base font-semibold">Demandez à votre assistant IA</h2>
        <p className="text-sm text-pretty text-muted-foreground">Essayez : « {prompt} »</p>
        <details className="mt-1 text-sm">
          <summary className="cursor-pointer text-muted-foreground hover:text-foreground">Brancher CallistSEO à Claude Code</summary>
          <div className="mt-2 flex items-center gap-1 rounded-lg border border-line bg-background px-2 py-1">
            <code className="min-w-0 flex-1 truncate text-xs">{command}</code>
            <CopyButton text={command} label="Copier la commande" />
          </div>
          <p className="mt-1 text-xs text-muted-foreground">Nécessite la commande `callistseo` (voir « Serveur MCP » dans le README) ; même base que le serveur pour retrouver vos audits.</p>
        </details>
      </div>
      <Button
        variant="outline"
        size="sm"
        className="justify-self-start md:mr-6"
        onClick={() =>
          void navigator.clipboard
            .writeText(prompt)
            .then(() => setCopied(true))
            .catch(() => {})
        }
      >
        {copied ? <Check aria-hidden /> : <Copy aria-hidden />}
        {copied ? 'Copiée' : 'Copier la demande'}
      </Button>
      <Button variant="ghost" size="icon-xs" className="absolute right-2 top-2" aria-label="Masquer la carte assistant" onClick={dismiss}>
        <X aria-hidden />
      </Button>
    </section>
  )
}
