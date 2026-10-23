export type ThemePreference = 'light' | 'dark' | 'system'

const KEY = 'callistseo.theme'
const VALUES: readonly ThemePreference[] = ['light', 'dark', 'system']

function parse(value: unknown): ThemePreference | null {
  return VALUES.includes(value as ThemePreference) ? (value as ThemePreference) : null
}

/** Thème effectif : « système » suit la préférence du système d'exploitation. */
export function resolveTheme(pref: ThemePreference, systemDark: boolean): 'light' | 'dark' {
  return pref === 'system' ? (systemDark ? 'dark' : 'light') : pref
}

/** `?theme=dark` (lien partageable) l'emporte sur la préférence mémorisée ; sinon « système ». */
export function pickPreference(search: string, stored: string | null): ThemePreference {
  return parse(new URLSearchParams(search).get('theme')) ?? parse(stored) ?? 'system'
}

/** Ordre de la bascule de l'en-tête. */
export function nextPreference(pref: ThemePreference): ThemePreference {
  return VALUES[(VALUES.indexOf(pref) + 1) % VALUES.length]
}

export function loadPreference(): ThemePreference {
  let stored: string | null = null
  try {
    stored = localStorage.getItem(KEY)
  } catch {
    // stockage indisponible (navigation privée) : on retombe sur « système »
  }
  return pickPreference(window.location.search, stored)
}

export function savePreference(pref: ThemePreference): void {
  try {
    localStorage.setItem(KEY, pref)
  } catch {
    // préférence non mémorisée : elle s'applique quand même à la session
  }
}

/** Applique le thème au document (classe `dark` lue par les variables de index.css). */
export function applyTheme(pref: ThemePreference): void {
  const systemDark = window.matchMedia('(prefers-color-scheme: dark)').matches
  const theme = resolveTheme(pref, systemDark)
  document.documentElement.classList.toggle('dark', theme === 'dark')
  document.documentElement.style.colorScheme = theme
}
