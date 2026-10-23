import { Button } from '@/components/ui/button'

interface Props {
  total: number
  limit: number
  offset: number
  onPage: (page: number) => void
}

export function Pagination({ total, limit, offset, onPage }: Props) {
  const page = Math.floor(offset / limit) + 1
  const from = total === 0 ? 0 : offset + 1
  const to = Math.min(offset + limit, total)
  return (
    <div className="flex items-center justify-between gap-4 py-3">
      <p className="text-sm text-muted-foreground">
        {from}–{to} sur {total}
      </p>
      <div className="flex gap-2">
        <Button variant="outline" size="sm" disabled={offset === 0} onClick={() => onPage(page - 1)}>
          Précédent
        </Button>
        <Button variant="outline" size="sm" disabled={offset + limit >= total} onClick={() => onPage(page + 1)}>
          Suivant
        </Button>
      </div>
    </div>
  )
}
