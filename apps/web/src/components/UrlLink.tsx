import { Link } from 'react-router-dom'
import { ExternalLink } from 'lucide-react'
import { CopyButton } from '@/components/CopyButton'
import { isDesktop, openExternal } from '@/lib/transport'
import { detailPath, isWebUrl, shortUrl } from '@/lib/url'
import { cn } from '@/lib/utils'

/**
 * Une URL de l'audit : texte lisible (tronqué, complet en infobulle), lien vers sa fiche si `crawlId`
 * est donné, et deux actions : ouvrir dans un nouvel onglet (navigateur système en desktop) et copier.
 * `compact` : actions visibles au survol seulement et hors ordre de tabulation (cellule du tableau).
 */
export function UrlLink({
  url,
  crawlId,
  compact = false,
  short = false,
  className,
}: {
  url: string
  crawlId?: number
  compact?: boolean
  /** Affiche le chemin seul (volet étroit) ; l'URL entière reste dans l'infobulle, le lien et la copie. */
  short?: boolean
  className?: string
}) {
  const web = isWebUrl(url)
  const text = short ? shortUrl(url) : url
  const reveal = compact ? 'opacity-0 group-hover/url:opacity-100 group-focus-within/url:opacity-100' : ''
  const tab = compact ? -1 : undefined
  return (
    <span className={cn('group/url flex min-w-0 items-center gap-1', className)}>
      {crawlId !== undefined ? (
        <Link to={detailPath(crawlId, url)} className="min-w-0 truncate font-mono text-sm underline-offset-4 hover:underline" title={url} tabIndex={tab} onClick={(e) => e.stopPropagation()}>
          {text}
        </Link>
      ) : (
        <span className="min-w-0 truncate font-mono text-sm" title={url}>
          {text}
        </span>
      )}
      <span className={cn('flex shrink-0 items-center transition-opacity', reveal)}>
        {web && (
          <a
            href={url}
            target="_blank"
            rel="noopener noreferrer"
            aria-label="Ouvrir dans un nouvel onglet"
            title="Ouvrir dans un nouvel onglet"
            tabIndex={tab}
            className="inline-flex size-6 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground [&_svg]:size-3.5"
            onClick={(e) => {
              e.stopPropagation()
              // Webview Tauri : un target=_blank n'ouvre pas le navigateur du système.
              if (isDesktop) {
                e.preventDefault()
                void openExternal(url)
              }
            }}
          >
            <ExternalLink aria-hidden />
          </a>
        )}
        <CopyButton text={url} label="Copier l'URL" tabIndex={tab} />
      </span>
    </span>
  )
}
