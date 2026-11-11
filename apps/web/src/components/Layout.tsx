import { useRef, useState } from 'react'
import { Link, Outlet, useMatch, useNavigate } from 'react-router-dom'
import { LogOut, Menu, Monitor, Moon, Sun } from 'lucide-react'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { Button } from '@/components/ui/button'
import { AppSidebar, type SidebarContext } from '@/components/AppSidebar'
import { UpdateBanner } from '@/components/UpdateBanner'
import { api, getToken, setToken } from '@/lib/api'
import { applyTheme, loadPreference, nextPreference, savePreference, type ThemePreference } from '@/lib/theme'
import { cn } from '@/lib/utils'
import { isDesktop } from '@/lib/transport'

/** Visible seulement une fois connecté (un serveur sans jeton n'a pas de connexion). */
function LogoutButton() {
  const queryClient = useQueryClient()
  const navigate = useNavigate()
  if (!getToken()) return null
  return (
    <Button
      variant="secondary"
      size="sm"
      onClick={() => {
        setToken('')
        queryClient.clear()
        navigate('/login')
      }}
    >
      <LogOut aria-hidden />
      Se déconnecter
    </Button>
  )
}

const THEME_LABEL: Record<ThemePreference, { label: string; Icon: typeof Sun }> = {
  light: { label: 'Clair', Icon: Sun },
  dark: { label: 'Sombre', Icon: Moon },
  system: { label: 'Auto', Icon: Monitor },
}

/** Bascule clair → sombre → auto (suit le système). */
function ThemeToggle() {
  const [pref, setPref] = useState<ThemePreference>(loadPreference)
  const { label, Icon } = THEME_LABEL[pref]
  return (
    <Button
      variant="outline"
      size="sm"
      aria-label={`Thème : ${label}. Cliquer pour changer.`}
      onClick={() => {
        const next = nextPreference(pref)
        setPref(next)
        savePreference(next)
        applyTheme(next)
      }}
    >
      <Icon aria-hidden />
      {label}
    </Button>
  )
}

/** Version du serveur (utile pour savoir quoi mettre à jour) ; rien si la sonde ne répond pas. */
function ServerVersion() {
  const health = useQuery({ queryKey: ['health'], queryFn: api.health, staleTime: Infinity })
  if (!health.data?.version) return null
  return <span className="text-xs text-muted-foreground tabular-nums">v{health.data.version}</span>
}

const SIDEBAR_KEY = 'callistseo.sidebar'

function storedCollapsed(): boolean | null {
  try {
    const v = localStorage.getItem(SIDEBAR_KEY)
    return v === null ? null : v === '1'
  } catch {
    return null
  }
}

/** Projet et audit ouverts, lus dans le cache des pages (mêmes clés : aucun appel de plus). */
function useSidebarContext(): SidebarContext {
  const crawlId = Number(useMatch('/crawls/:id/*')?.params.id ?? NaN)
  const projectId = Number(useMatch('/projects/:id')?.params.id ?? NaN)
  const crawl = useQuery({ queryKey: ['crawl', crawlId], queryFn: () => api.crawl(crawlId), enabled: Number.isFinite(crawlId) })
  const project = useQuery({ queryKey: ['project', projectId], queryFn: () => api.project(projectId), enabled: Number.isFinite(projectId) })
  if (Number.isFinite(crawlId) && crawl.data) {
    const c = crawl.data
    return { project: { id: c.project_id, name: c.project }, crawl: { id: c.id, running: c.status === 'running' } }
  }
  if (Number.isFinite(projectId) && project.data) return { project: { id: projectId, name: project.data.name } }
  return {}
}

export function Layout() {
  // L'écran d'un audit est un poste de travail (rapports, tableau, inspecteur) : il prend toute la largeur.
  const wide = useMatch('/crawls/:id') !== null
  const context = useSidebarContext()
  // Repliée par défaut sur le poste de travail (il a déjà sa colonne de rapports), sauf choix mémorisé.
  const [stored, setStored] = useState(storedCollapsed)
  const collapsed = stored ?? wide
  const drawer = useRef<HTMLDialogElement>(null)
  const footer = (
    <div className="flex flex-wrap items-center gap-2 px-1">
      <ThemeToggle />
      {/* En desktop l'API n'écoute sur aucun port : pas de connexion. */}
      {!isDesktop && <LogoutButton />}
      <span className="ml-auto">
        <ServerVersion />
      </span>
    </div>
  )
  const toggle = () => {
    setStored(!collapsed)
    try {
      localStorage.setItem(SIDEBAR_KEY, collapsed ? '0' : '1')
    } catch {
      // stockage indisponible (navigation privée) : le choix vaut pour la session
    }
  }

  return (
    <div className={cn('min-h-svh text-foreground lg:flex', wide && 'lg:h-svh lg:overflow-hidden')}>
      <a
        href="#contenu"
        className="sr-only focus:not-sr-only focus:fixed focus:left-3 focus:top-3 focus:z-50 focus:rounded-lg focus:border focus:border-line focus:bg-card focus:px-3 focus:py-2 focus:text-sm focus:font-semibold focus:shadow-pop"
      >
        Aller au contenu
      </a>
      <AppSidebar context={context} collapsed={collapsed} onToggle={toggle} footer={footer} className="sticky top-0 hidden h-svh shrink-0 overflow-y-auto lg:flex" />
      {/* Tiroir mobile : <dialog> modal (focus piégé, Échap) ; se ferme aussi sur le fond ou en suivant un lien. */}
      <dialog
        ref={drawer}
        aria-label="Navigation"
        className="m-0 h-full max-h-none overscroll-contain bg-transparent p-0 backdrop:bg-foreground/30 lg:hidden"
        onClick={(e) => (e.target === e.currentTarget || (e.target as HTMLElement).closest('a')) && drawer.current?.close()}
      >
        <AppSidebar context={context} collapsed={false} footer={footer} className="h-full overflow-y-auto" />
      </dialog>
      <div className={cn('flex min-w-0 flex-1 flex-col', wide && 'lg:min-h-0')}>
        {/* Grand écran : la barre latérale situe la page (projet, section) ; ici seulement le menu mobile. */}
        <header className="sticky top-0 z-20 flex shrink-0 items-center gap-2 border-b border-line bg-card/90 px-4 py-2 backdrop-blur lg:hidden">
          <Button variant="ghost" size="icon-sm" aria-label="Ouvrir la navigation" aria-haspopup="dialog" onClick={() => drawer.current?.showModal()}>
            <Menu aria-hidden />
          </Button>
          <Link to="/" className="font-heading font-extrabold tracking-tight">CallistSEO</Link>
        </header>
        {isDesktop && <UpdateBanner />}
        <main id="contenu" tabIndex={-1} className={cn('outline-none', 'mx-auto w-full px-4', wide ? 'py-4 lg:min-h-0 lg:flex-1' : 'max-w-7xl py-8 lg:px-10')}>
          <Outlet />
        </main>
      </div>
    </div>
  )
}
