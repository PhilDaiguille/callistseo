// Client typé de l'API (voir README) : HTTP via `callistseo serve`, ou en processus dans l'application desktop.
import { apiFetch, isDesktop, saveDownload } from './transport'

export type RunStatus = 'running' | 'done' | 'failed' | 'cancelled'
export type Severity = 'Error' | 'Warning' | 'Info'

export interface Project {
  id: number
  name: string
  start_url: string
  created_at: number
  /** Dernier audit du projet (liste d'accueil seulement). */
  last_crawl?: Crawl | null
}

export interface Crawl {
  id: number
  project_id: number
  project: string
  start_url: string
  status: RunStatus
  started_at: number
  finished_at: number | null
  error: string | null
  pages: number
  errors: number
  warnings: number
  /** Score de santé (0-100) : part des URL sans erreur. `null` tant que l'audit n'est pas terminé. */
  health: number | null
  /** Présent seulement pendant l'exécution. */
  live?: LiveStats
}

export interface LiveStats {
  in_flight: number
  concurrency: number
  queued: number
  /** Limite de pages de l'audit (0 : inconnue). */
  max_pages: number
  urls_per_second: number
  avg_latency_ms: number
  failures: number
  paused: boolean
  stopping: boolean
}

export interface ProjectDetail extends Project {
  crawls: Crawl[]
}

export interface PageItem {
  url: string
  depth: number
  status: number | null
  redirect_to: string | null
  title: string | null
  description: string | null
  h1: string[]
  canonical: string | null
  robots: string | null
  in_sitemap: boolean
  is_html: boolean
  error: string | null
  /** Absent des anciens audits (avant le schéma v3) : valeurs nulles ou vides. */
  facts?: PageFacts
}

export interface PageFacts {
  ttfb_ms: number | null
  total_ms: number | null
  size: number | null
  content_type: string | null
  headers: [string, string][]
  content_hash: string | null
  word_count: number | null
  lang: string | null
  viewport: string | null
  h2: string[]
  hreflang: [string, string][]
  images: [string, string | null][]
  images_total: number
  json_ld: string[]
  nofollow_links: number
  extracted: Record<string, string[]>
}

export interface IssueItem {
  severity: Severity
  rule: string
  url: string
  message: string
}

export interface Paged<T> {
  total: number
  limit: number
  offset: number
  items: T[]
}

export interface IssuesPage extends Paged<IssueItem> {
  summary: { rule: string; severity: Severity; count: number }[]
}

export interface DiffChange {
  field: string
  before: string
  after: string
}

export interface DiffResult {
  before: Crawl
  after: Crawl
  counts: {
    added_pages: number
    removed_pages: number
    changed_pages: number
    new_issues: number
    resolved_issues: number
    new_errors: number
  }
  added_pages: string[]
  removed_pages: string[]
  changed_pages: { url: string; changes: DiffChange[] }[]
  new_issues: IssueItem[]
  resolved_issues: IssueItem[]
  /** Les listes sont plafonnées côté serveur ; les compteurs restent exacts. */
  truncated: boolean
}

export interface PageDetail {
  page: PageItem
  issues: IssueItem[]
  inlinks: { total: number; items: { url: string; status: number | null; title: string | null }[] }
  outlinks: { total: number; items: { url: string; crawled: boolean; status: number | null }[] }
}

export interface SegmentDef {
  url_contains?: string | null
  url_prefix?: string | null
  status_class?: number | null
  max_depth?: number | null
  issue_rule?: string | null
  has_extracted?: string | null
}

export interface Segment {
  id: number
  name: string
  definition: SegmentDef
}

export interface SegmentCount {
  id: number
  name: string
  count: number
}

export interface ReportEntry {
  id: string
  label: string
  count: number
  columns: string[]
}

export interface ReportGroup {
  label: string
  reports: ReportEntry[]
}

export interface IssueReport {
  /** `issue:<règle>` : identifiant utilisable comme rapport. */
  id: string
  rule: string
  label: string
  severity: Severity
  count: number
}

export interface ColumnDef {
  id: string
  label: string
  numeric: boolean
}

export interface ReportsResponse {
  groups: ReportGroup[]
  issues: IssueReport[]
  columns: ColumnDef[]
}

export type Cell = string | number | null

export interface RowsResponse {
  total: number
  columns: string[]
  rows: Cell[][]
  limit: number
  offset: number
}

export interface RowsParams {
  report: string
  sort?: string
  dir?: 'asc' | 'desc'
  columns?: string[]
  search?: string
  segment?: number
  limit: number
  offset: number
}

/** Configuration d'audit d'un projet (voir `PUT /api/projects/{id}/config`). */
export interface ExtractorDef {
  name: string
  kind: 'regex' | 'css' | 'contains' | 'xpath'
  pattern: string
  attr: string | null
}

export interface ProjectConfig {
  include: string[]
  exclude: string[]
  strip_params: string[]
  max_url_length: number | null
  robots_txt: string | null
  max_pages: number | null
  max_depth: number | null
  delay_ms: number | null
  concurrency: number | null
  extractors: ExtractorDef[]
  integrations?: IntegrationsConfig
}

/** Intégrations externes, activées par projet (une intégration absente est désactivée). */
export interface IntegrationsConfig {
  psi?: { strategy: 'mobile' | 'desktop'; max_urls: number }
  gsc?: { property: string }
  ga4?: { property_id: string }
}

/** État des clés d'intégrations : jamais leurs valeurs. */
export interface IntegrationKeysState {
  available: boolean
  readable: boolean
  psi_key: boolean
  service_account: string | null
}

export interface IntegrationKeysBody {
  psi_key?: string
  service_account?: string
}

/** État des identifiants d'un projet : jamais leurs valeurs. */
export interface SiteAuthState {
  available: boolean
  configured: boolean
  readable: boolean
  basic: boolean
  headers: number
  cookies: boolean
  form: boolean
}

export interface SiteAuthBody {
  basic?: { user: string; password: string }
  headers: { name: string; value: string }[]
  cookies?: string
  form?: { url: string; username: string; password: string; username_field?: string; password_field?: string }
}

export interface Schedule {
  interval_minutes: number
  enabled: boolean
  next_run_at: number
  last_run_id: number | null
}

export interface LinksOverview {
  internal_links: number
  depths: { depth: number; count: number }[]
  top_linked: { url: string; count: number }[]
  top_linking: { url: string; count: number }[]
  orphans: { total: number; items: string[] }
}

export interface CrawlOptions {
  /** Délai minimal entre deux requêtes, en millisecondes. */
  delay_ms?: number
  max_pages?: number
  max_depth?: number
  concurrency?: number
}

export class ApiError extends Error {
  status: number

  constructor(status: number, message: string) {
    super(message)
    this.status = status
  }
}

const TOKEN_KEY = 'callistseo.token'

// Mot de passe de l'équipe (= jeton du serveur) : pour l'onglet (sessionStorage) ou, avec
// « Se souvenir de moi », pour ce poste (localStorage). L'app fonctionne sans.
export function getToken(): string {
  try {
    return localStorage.getItem(TOKEN_KEY) ?? sessionStorage.getItem(TOKEN_KEY) ?? ''
  } catch {
    return ''
  }
}

export function setToken(token: string, remember = false): void {
  try {
    localStorage.removeItem(TOKEN_KEY)
    sessionStorage.removeItem(TOKEN_KEY)
    if (token) (remember ? localStorage : sessionStorage).setItem(TOKEN_KEY, token)
  } catch {
    // stockage indisponible (navigation privée, etc.) : le jeton reste simplement non mémorisé
  }
}

/** Adresse de retour après connexion : chemin interne uniquement (pas de redirection ouverte). */
export function safeNext(next: string | null): string {
  return next && next.startsWith('/') && !next.startsWith('//') && !next.startsWith('/\\') ? next : '/'
}

/** Page de connexion à ouvrir après un 401, ou `null` si on y est déjà. */
export function loginPath(pathname: string, search: string): string | null {
  if (pathname === '/login') return null
  return `/login?next=${encodeURIComponent(pathname + search)}`
}

/** Chaîne de requête sans les valeurs vides. */
export function qs(params: Record<string, string | number | null | undefined>): string {
  const sp = new URLSearchParams()
  for (const [k, v] of Object.entries(params)) {
    if (v !== undefined && v !== null && v !== '') sp.set(k, String(v))
  }
  const s = sp.toString()
  return s ? `?${s}` : ''
}

function authHeaders(): Headers {
  const headers = new Headers()
  const token = getToken()
  if (token) headers.set('Authorization', `Bearer ${token}`)
  return headers
}

async function fail(res: Response): Promise<never> {
  // Accès refusé : direction la page de connexion (jamais en desktop, qui n'a pas de jeton).
  if (res.status === 401 && !isDesktop) {
    const to = loginPath(window.location.pathname, window.location.search)
    if (to) window.location.assign(to)
  }
  const body = (await res.json().catch(() => null)) as { error?: string } | null
  throw new ApiError(res.status, body?.error ?? `Erreur HTTP ${res.status}`)
}

async function request<T>(path: string, init: { method?: string; body?: unknown } = {}): Promise<T> {
  const headers = authHeaders()
  if (init.body !== undefined) headers.set('Content-Type', 'application/json')
  const res = await apiFetch(path, {
    method: init.method,
    headers,
    body: init.body === undefined ? undefined : JSON.stringify(init.body),
  })
  if (!res.ok) await fail(res)
  if (res.status === 204) return null as T
  return (await res.json()) as T
}

export const api = {
  /** Publique : état et version du serveur. */
  health: () => request<{ status: string; version: string }>('/api/health'),
  projects: () => request<{ projects: Project[] }>('/api/projects').then((r) => r.projects),
  createProject: (body: { name: string; start_url: string }) =>
    request<Project>('/api/projects', { method: 'POST', body }),
  deleteProject: (id: number) => request<null>(`/api/projects/${id}`, { method: 'DELETE' }),
  project: (id: number) => request<ProjectDetail>(`/api/projects/${id}`),
  startCrawl: (projectId: number, body: CrawlOptions = {}) =>
    request<Crawl>(`/api/projects/${projectId}/crawls`, { method: 'POST', body }),
  projectConfig: (id: number) => request<ProjectConfig>(`/api/projects/${id}/config`),
  saveProjectConfig: (id: number, body: ProjectConfig) =>
    request<ProjectConfig>(`/api/projects/${id}/config`, { method: 'PUT', body }),
  schedule: (id: number) => request<Schedule | null>(`/api/projects/${id}/schedule`),
  saveSchedule: (id: number, body: { interval_minutes: number; enabled: boolean }) =>
    request<Schedule>(`/api/projects/${id}/schedule`, { method: 'PUT', body }),
  siteAuth: (id: number) => request<SiteAuthState>(`/api/projects/${id}/auth`),
  saveSiteAuth: (id: number, body: SiteAuthBody) => request<null>(`/api/projects/${id}/auth`, { method: 'PUT', body }),
  deleteSiteAuth: (id: number) => request<null>(`/api/projects/${id}/auth`, { method: 'DELETE' }),
  integrationKeys: (id: number) => request<IntegrationKeysState>(`/api/projects/${id}/integrations/keys`),
  saveIntegrationKeys: (id: number, body: IntegrationKeysBody) =>
    request<null>(`/api/projects/${id}/integrations/keys`, { method: 'PUT', body }),
  deleteSchedule: (id: number) => request<null>(`/api/projects/${id}/schedule`, { method: 'DELETE' }),
  crawl: (id: number) => request<Crawl>(`/api/crawls/${id}`),
  diff: (before: number, after: number) => request<DiffResult>(`/api/crawls/${before}/diff/${after}`),
  pageDetail: (id: number, url: string) => request<PageDetail>(`/api/crawls/${id}/page${qs({ url })}`),
  links: (id: number) => request<LinksOverview>(`/api/crawls/${id}/links`),
  reports: (id: number) => request<ReportsResponse>(`/api/crawls/${id}/reports`),
  rows: (id: number, p: RowsParams) =>
    request<RowsResponse>(`/api/crawls/${id}/rows${qs({ ...p, columns: p.columns?.join(',') })}`),
  segments: (crawlId: number) => request<{ segments: SegmentCount[] }>(`/api/crawls/${crawlId}/segments`),
  projectSegments: (projectId: number) => request<{ segments: Segment[] }>(`/api/projects/${projectId}/segments`),
  createSegment: (projectId: number, body: { name: string; definition: SegmentDef }) =>
    request<Segment>(`/api/projects/${projectId}/segments`, { method: 'POST', body }),
  deleteSegment: (id: number) => request<null>(`/api/segments/${id}`, { method: 'DELETE' }),
  control: (id: number, action: 'pause' | 'resume' | 'stop') =>
    request<Crawl>(`/api/crawls/${id}/${action}`, { method: 'POST' }),
  cancel: (id: number) => request<Crawl>(`/api/crawls/${id}/cancel`, { method: 'POST' }),
  pages: (id: number, p: { search?: string; status?: string; limit: number; offset: number }) =>
    request<Paged<PageItem>>(`/api/crawls/${id}/pages${qs(p)}`),
  issues: (id: number, p: { severity?: string; rule?: string; limit: number; offset: number }) =>
    request<IssuesPage>(`/api/crawls/${id}/issues${qs(p)}`),
}

/** Télécharge un export (le jeton Bearer interdit un simple lien `<a href>`). */
export async function downloadExport(
  id: number,
  format: 'csv' | 'json' | 'html',
  kind: 'pages' | 'issues' = 'pages',
): Promise<void> {
  const res = await apiFetch(`/api/crawls/${id}/export${qs({ format, kind: format === 'csv' ? kind : undefined })}`, {
    headers: authHeaders(),
  })
  if (!res.ok) await fail(res)
  await saveDownload(format === 'csv' ? `crawl-${id}-${kind}.csv` : `crawl-${id}.${format}`, res)
}

/** Sitemap XML des pages indexables de l'audit (premier fichier ; 50 000 URL max). */
export async function downloadSitemap(id: number): Promise<void> {
  const res = await apiFetch(`/api/crawls/${id}/sitemap.xml`, { headers: authHeaders() })
  if (!res.ok) await fail(res)
  await saveDownload(`sitemap-${id}.xml`, res)
}
