import { Badge } from '@/components/ui/badge'
import type { RunStatus, Severity } from '@/lib/api'
import { statusTone } from '@/lib/format'
import { SEVERITY_LABEL } from '@/lib/rules'

// Les couleurs fortes sont réservées aux vrais problèmes (erreurs, échecs).
export function HttpStatus({ status, error }: { status: number | null; error?: string | null }) {
  const tone = statusTone(status)
  if (tone === 'none') {
    if (error?.includes('robots.txt')) return <Badge variant="outline">robots</Badge>
    return <Badge variant={error ? 'destructive' : 'outline'}>{error ? 'ERR' : '—'}</Badge>
  }
  return <Badge variant={tone === 'error' ? 'destructive' : tone === 'redirect' ? 'warning' : 'success'}>{status}</Badge>
}

const RUN_LABEL: Record<RunStatus, string> = {
  running: 'En cours',
  done: 'Terminé',
  failed: 'Échec',
  cancelled: 'Annulé',
}

export function RunBadge({ status }: { status: RunStatus }) {
  const variant = status === 'failed' ? 'destructive' : status === 'running' ? 'info' : status === 'done' ? 'success' : 'outline'
  return (
    <Badge variant={variant} className={status === 'running' ? 'animate-pulse' : undefined}>
      {RUN_LABEL[status]}
    </Badge>
  )
}

export function SeverityBadge({ severity }: { severity: Severity }) {
  const variant = severity === 'Error' ? 'destructive' : severity === 'Warning' ? 'warning' : 'info'
  return <Badge variant={variant}>{SEVERITY_LABEL[severity]}</Badge>
}
