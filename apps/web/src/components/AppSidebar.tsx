import { Link, useLocation } from 'react-router-dom'
import {
  CalendarClock,
  FolderKanban,
  GitCompareArrows,
  History,
  LayoutDashboard,
  Network,
  PanelLeftClose,
  PanelLeftOpen,
  Settings2,
  Table2,
  type LucideIcon,
} from 'lucide-react'
import { Mascot } from '@/components/Mascot'
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
  disabled?: boolean
}

/**
 * Navigation globale : projets, puis le projet et l'audit ouverts. Les liens reprennent les
 * paramètres d'adresse existants (`?tab=`, `?r=`) : aucune nouvelle route.
 */
export function AppSidebar({
  context,
  collapsed,
  onToggle,
  className,
}: {
  context: SidebarContext
  collapsed: boolean
  onToggle?: () => void
  className?: string
}) {
  const { pathname, search } = useLocation()
  const params = new URLSearchParams(search)
  const tab = params.get('tab')
  const { project, crawl } = context

  const sections: { title?: string; items: Item[] }[] = [
    { items: [{ to: '/', label: 'Projets', Icon: FolderKanban, active: pathname === '/' }] },
  ]
  if (project) {
    const base = `/projects/${project.id}`
    const here = pathname === base
    sections.push({
      title: project.name,
      items: [
        { to: base, label: 'Audits', Icon: History, active: here && !tab },
        { to: `${base}?tab=schedule`, label: 'Planification', Icon: CalendarClock, active: here && tab === 'schedule' },
        { to: `${base}?tab=config`, label: 'Configuration', Icon: Settings2, active: here && tab === 'config' },
      ],
    })
  }
  if (crawl) {
    const base = `/crawls/${crawl.id}`
    const here = pathname === base
    const report = params.get('r') ?? (crawl.running ? 'all' : 'overview')
    sections.push({
      title: `Audit #${crawl.id}`,
      items: [
        { to: base, label: 'Vue d’ensemble', Icon: LayoutDashboard, active: here && !tab && report === 'overview', disabled: crawl.running },
        { to: `${base}?r=all`, label: 'Toutes les URL', Icon: Table2, active: here && !tab && report === 'all' },
        { to: `${base}?tab=links`, label: 'Maillage', Icon: Network, active: here && tab === 'links', disabled: crawl.running },
        { to: `${base}?tab=diff`, label: 'Comparaison', Icon: GitCompareArrows, active: here && tab === 'diff', disabled: crawl.running },
      ],
    })
  }

  return (
    <nav aria-label="Navigation principale" className={cn('flex flex-col gap-4 border-r border-line bg-card p-3', collapsed ? 'w-16 items-center' : 'w-60', className)}>
      <Link to="/" className="flex items-center gap-2 px-1 font-heading text-lg font-extrabold tracking-tight" aria-label="CallistSEO, accueil">
        <Mascot size={32} />
        {!collapsed && 'CallistSEO'}
      </Link>
      {sections.map((s, i) => (
        <div key={i} className="grid w-full gap-0.5">
          {s.title &&
            (collapsed ? (
              <hr className="mx-2 mb-1 border-line" />
            ) : (
              <p className="truncate px-2 pb-1 text-xs font-semibold tracking-wide text-muted-foreground uppercase" title={s.title}>
                {s.title}
              </p>
            ))}
          {s.items.map((item) => (
            <NavItem key={item.to} item={item} collapsed={collapsed} />
          ))}
        </div>
      ))}
      {onToggle && (
        <button
          type="button"
          onClick={onToggle}
          aria-label={collapsed ? 'Déplier la navigation' : 'Replier la navigation'}
          aria-expanded={!collapsed}
          className="mt-auto flex items-center gap-2 rounded-lg px-2 py-1.5 text-sm text-muted-foreground hover:bg-accent hover:text-foreground"
        >
          {collapsed ? <PanelLeftOpen className="size-4" aria-hidden /> : <PanelLeftClose className="size-4" aria-hidden />}
          {!collapsed && 'Replier'}
        </button>
      )}
    </nav>
  )
}

function NavItem({ item, collapsed }: { item: Item; collapsed: boolean }) {
  const { to, label, Icon, active, disabled } = item
  const cls = cn(
    'flex items-center gap-2.5 rounded-lg px-2 py-1.5 text-sm font-medium transition-colors',
    collapsed && 'justify-center',
    active ? 'bg-primary text-primary-foreground' : 'text-muted-foreground hover:bg-accent hover:text-foreground',
  )
  const content = (
    <>
      <Icon className="size-4 shrink-0" aria-hidden />
      {collapsed ? <span className="sr-only">{label}</span> : label}
    </>
  )
  // Vue indisponible pendant un audit (problèmes et maillage sont calculés à la fin).
  if (disabled) {
    return (
      <span aria-disabled="true" title={`${label} (après l’audit)`} className={cn(cls, 'cursor-not-allowed opacity-50 hover:bg-transparent hover:text-muted-foreground')}>
        {content}
      </span>
    )
  }
  return (
    <Link to={to} aria-current={active ? 'page' : undefined} title={collapsed ? label : undefined} className={cls}>
      {content}
    </Link>
  )
}
