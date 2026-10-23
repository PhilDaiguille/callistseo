import { Fragment } from 'react'
import { UrlLink } from '@/components/UrlLink'
import type { PageItem } from '@/lib/api'

/** Métadonnées observées sur une page. */
export function PageDetails({ page }: { page: PageItem }) {
  const urls = new Set(['Canonical', 'Redirige vers'])
  const rows: [string, string | null][] = [
    ['Title', page.title],
    ['Description', page.description],
    ['H1', page.h1.length ? page.h1.join(' · ') : null],
    ['Canonical', page.canonical],
    ['Meta robots', page.robots],
    ['Redirige vers', page.redirect_to],
    ['Erreur', page.error],
    ['Sitemap', page.in_sitemap ? 'oui' : 'non'],
  ]
  return (
    <dl className="grid gap-x-6 gap-y-1 text-sm sm:grid-cols-[8rem_1fr]">
      {rows.map(([label, value]) => (
        <Fragment key={label}>
          <dt className="text-muted-foreground">{label}</dt>
          <dd className="min-w-0 break-all">
            {value === null ? <span className="text-muted-foreground">—</span> : urls.has(label) ? <UrlLink url={value} /> : value}
          </dd>
        </Fragment>
      ))}
    </dl>
  )
}
