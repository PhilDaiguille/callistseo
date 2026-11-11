import type { ReactNode } from 'react'
import { Link, useLocation, useNavigate } from 'react-router-dom'
import { useQuery } from '@tanstack/react-query'
import {
  CalendarClock,
  FolderKanban,
  History,
  PanelLeftClose,
  PanelLeftOpen,
  Settings,
  Settings2,
  type LucideIcon,
} from 'lucide-react'
import { Mascot } from '@/components/Mascot'
import { Select, SelectContent, SelectItem, SelectTrigger } from '@/components/ui/select'
import { api } from '@/lib/api'
import { cn } from '@/lib/utils'

export interface SidebarContext {
  project?: { id: number; name: string }
  crawl?: { id: number; running: boolean }
}

interface Item {
  to: string
  label: string
  Icon: LucideIcon
  active: boolean
}

/** Domaine affiché sous le nom du projet ; l'URL brute si elle ne se lit pas. */
function host(url: string): string {
  try {
    return new URL(url).host
  } catch {
    return url
  }
}

/**
 * Navigation globale : sélecteur de projet, puis sections du projet ouvert (les vues d'un audit
 * sont ses onglets). Les liens reprennent les paramètres d'adresse existants (`?tab=`).
 */
export function AppSidebar({
  context,
  collapsed,
  onToggle,
  footer,
  className,
}: {
  context: SidebarContext
  collapsed: boolean
  onToggle?: () => void
  /** Version, thème, déconnexion : en bas de la barre. */
  footer?: ReactNode
  className?: string
}) {
  const { pathname, search } = useLocation()
  const tab = new URLSearchParams(search).get('tab')
  const { project } = context

  const sections: { title: string; items: Item[] }[] = [
    { title: 'Vue d’ensemble', items: [{ to: '/', label: 'Projets', Icon: FolderKanban, active: pathname === '/' }] },
  ]
  if (project) {
    const base = `/projects/${project.id}`
    const here = pathname === base
    sections.push(
      {
        title: 'Projet',
        items: [
          // Un audit ouvert appartient à « Audits ».
          { to: base, label: 'Audits', Icon: History, active: (here && !tab) || pathname.startsWith('/crawls/') },
          { to: `${base}?tab=schedule`, label: 'Planification', Icon: CalendarClock, active: here && tab === 'schedule' },
        ],
      },
      {
        title: 'Réglages',
        items: [{ to: `${base}?tab=config`, label: 'Configuration', Icon: Settings2, active: here && tab === 'config' }],
      },
    )
  }

  return (
    <nav aria-label="Navigation principale" className={cn('flex flex-col gap-5 border-r border-line bg-muted p-3', collapsed ? 'w-16 items-center' : 'w-64', className)}>
      <Link to="/" className="flex items-center gap-2 px-1 font-heading text-lg font-extrabold tracking-tight" aria-label="CallistSEO, accueil">
        <Mascot size={32} />
        {!collapsed && 'CallistSEO'}
      </Link>
      {!collapsed && <ProjectSwitcher current={project} />}
      {sections.map((s) => (
        <div key={s.title} className="grid w-full gap-0.5">
          {collapsed ? (
            <hr className="mx-2 mb-1 border-line" />
          ) : (
            <p className="px-2 pb-1 text-[11px] font-semibold uppercase tracking-wider text-muted-foreground">{s.title}</p>
          )}
          {s.items.map((item) => (
            <NavItem key={item.to} item={item} collapsed={collapsed} />
          ))}
        </div>
      ))}
      <div className={cn('mt-auto grid w-full gap-2 border-t border-line pt-3', collapsed && 'justify-items-center')}>
        {!collapsed && footer}
        {onToggle && (
          <button
            type="button"
            onClick={onToggle}
            aria-label={collapsed ? 'Déplier la navigation' : 'Replier la navigation'}
            aria-expanded={!collapsed}
            className="flex items-center gap-2 rounded-lg px-2 py-1.5 text-sm text-muted-foreground hover:bg-accent hover:text-foreground"
          >
            {collapsed ? <PanelLeftOpen className="size-4" aria-hidden /> : <PanelLeftClose className="size-4" aria-hidden />}
            {!collapsed && 'Replier'}
          </button>
        )}
      </div>
    </nav>
  )
}

/** Carte du projet ouvert : menu pour changer de projet, roue dentée vers la configuration. */
function ProjectSwitcher({ current }: { current?: { id: number; name: string } }) {
  const navigate = useNavigate()
  // Même clé que la page Projets : liste partagée, aucun appel en double.
  const projects = useQuery({ queryKey: ['projects'], queryFn: api.projects })
  const list = projects.data ?? []
  const url = list.find((p) => p.id === current?.id)?.start_url
  return (
    <div className="panel flex w-full items-stretch overflow-hidden">
      <Select value={current ? String(current.id) : null} onValueChange={(v) => v && navigate(`/projects/${v}`)}>
        <SelectTrigger aria-label="Changer de projet" className="h-auto min-w-0 flex-1 rounded-none data-[size=default]:h-auto border-0 bg-transparent px-3 py-2 hover:bg-accent dark:bg-transparent">
          <span className="grid min-w-0 text-left">
            <span className="truncate text-sm font-semibold">{current?.name ?? 'Choisir un projet'}</span>
            {url && <span className="truncate text-xs text-muted-foreground">{host(url)}</span>}
          </span>
        </SelectTrigger>
        <SelectContent alignItemWithTrigger={false} align="start" className="min-w-56">
          {list.map((p) => (
            <SelectItem key={p.id} value={String(p.id)}>
              {p.name}
            </SelectItem>
          ))}
          {list.length === 0 && <p className="px-2 py-1.5 text-sm text-muted-foreground">Aucun projet</p>}
        </SelectContent>
      </Select>
      {current && (
        <Link
          to={`/projects/${current.id}?tab=config`}
          aria-label="Configuration du projet"
          title="Configuration du projet"
          className="flex items-center border-l border-line px-3 text-muted-foreground hover:bg-accent hover:text-foreground"
        >
          <Settings className="size-4" aria-hidden />
        </Link>
      )}
    </div>
  )
}

function NavItem({ item, collapsed }: { item: Item; collapsed: boolean }) {
  const { to, label, Icon, active } = item
  const cls = cn(
    'relative flex items-center gap-2.5 rounded-lg px-2 py-1.5 text-sm font-medium transition-colors',
    collapsed && 'justify-center',
    // Actif : fond léger + barre à gauche (pas de pavé plein).
    active
      ? 'bg-accent text-foreground before:absolute before:inset-y-1.5 before:-left-3 before:w-0.5 before:rounded-full before:bg-ring'
      : 'text-muted-foreground hover:bg-accent hover:text-foreground',
  )
  return (
    <Link to={to} aria-current={active ? 'page' : undefined} title={collapsed ? label : undefined} className={cls}>
      <Icon className="size-4 shrink-0" aria-hidden />
      {collapsed ? <span className="sr-only">{label}</span> : label}
    </Link>
  )
}
