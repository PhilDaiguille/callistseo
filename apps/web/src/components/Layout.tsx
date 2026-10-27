import { useState } from 'react'
import { Link, NavLink, Outlet, useMatch, useNavigate } from 'react-router-dom'
import { LogOut, Monitor, Moon, Sun } from 'lucide-react'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { Button } from '@/components/ui/button'
import { Mascot } from '@/components/Mascot'
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

const navLink = ({ isActive }: { isActive: boolean }) =>
  cn(
    'rounded-xl border-2 px-3 py-1 text-sm font-semibold transition-colors',
    isActive ? 'border-ink bg-primary text-primary-foreground' : 'border-transparent text-muted-foreground hover:bg-accent hover:text-foreground',
  )

/** Version du serveur (utile pour savoir quoi mettre à jour) ; rien si la sonde ne répond pas. */
function ServerVersion() {
  const health = useQuery({ queryKey: ['health'], queryFn: api.health, staleTime: Infinity })
  if (!health.data?.version) return null
  return <span className="text-xs text-muted-foreground tabular-nums">v{health.data.version}</span>
}

export function Layout() {
  // L'écran d'un audit est un poste de travail (rapports, tableau, inspecteur) : il prend toute la largeur.
  const wide = useMatch('/crawls/:id') !== null
  return (
    <div className={cn('min-h-svh text-foreground', wide && 'lg:flex lg:h-svh lg:flex-col lg:overflow-hidden')}>
      <a
        href="#contenu"
        className="sr-only focus:not-sr-only focus:fixed focus:left-3 focus:top-3 focus:z-50 focus:rounded-lg focus:border-2 focus:border-ink focus:bg-card focus:px-3 focus:py-2 focus:text-sm focus:font-semibold"
      >
        Aller au contenu
      </a>
      <header className="shrink-0 border-b-2 border-ink bg-card">
        <div className={cn('mx-auto flex flex-wrap items-center gap-x-6 gap-y-2 px-4 py-3', wide ? 'max-w-none' : 'max-w-7xl')}>
          <Link to="/" className="flex items-center gap-2 font-heading text-xl font-extrabold tracking-tight">
            <Mascot size={36} />
            CallistSEO
          </Link>
          <nav className="flex gap-2">
            <NavLink to="/" end className={navLink}>
              Projets
            </NavLink>
          </nav>
          <div className="ml-auto flex items-center gap-2">
            <ServerVersion />
            <ThemeToggle />
            {/* En desktop l'API n'écoute sur aucun port : pas de connexion. */}
            {!isDesktop && <LogoutButton />}
          </div>
        </div>
      </header>
      <main id="contenu" tabIndex={-1} className={cn('outline-none', 'mx-auto w-full px-4', wide ? 'py-4 lg:min-h-0 lg:flex-1' : 'max-w-7xl py-8')}>
        <Outlet />
      </main>
    </div>
  )
}
