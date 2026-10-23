import { useEffect, useRef, useState } from 'react'
import { Search } from 'lucide-react'
import { matches, wrap } from '@/lib/table'
import { cn } from '@/lib/utils'

export interface Command {
  id: string
  label: string
  hint?: string
  run: () => void
}

/** Palette de commandes (⌘K / Ctrl+K) : élément `<dialog>` natif, donc focus piégé et Échap gérés. */
export function CommandPalette({ commands, open, onOpenChange }: { commands: Command[]; open: boolean; onOpenChange: (o: boolean) => void }) {
  const dialog = useRef<HTMLDialogElement>(null)
  const [query, setQuery] = useState('')
  const [index, setIndex] = useState(0)
  const found = commands.filter((c) => matches(query, `${c.label} ${c.hint ?? ''}`)).slice(0, 50)

  useEffect(() => {
    const d = dialog.current
    if (!d) return
    if (open && !d.open) {
      setQuery('')
      setIndex(0)
      d.showModal()
    } else if (!open && d.open) d.close()
  }, [open])

  const run = (c: Command | undefined) => {
    if (!c) return
    onOpenChange(false)
    c.run()
  }

  return (
    <dialog
      ref={dialog}
      aria-label="Palette de commandes"
      onClose={() => onOpenChange(false)}
      onClick={(e) => e.target === dialog.current && onOpenChange(false)}
      className="panel-pop m-auto mt-[12vh] w-[min(36rem,calc(100vw-2rem))] p-0 backdrop:bg-ink/40"
    >
      <div className="flex items-center gap-2 border-b border-line px-3">
        <Search className="size-4 text-muted-foreground" aria-hidden />
        <input
          autoFocus
          role="combobox"
          aria-expanded
          aria-controls="palette-list"
          aria-activedescendant={found[index] ? `cmd-${found[index].id}` : undefined}
          value={query}
          onChange={(e) => {
            setQuery(e.target.value)
            setIndex(0)
          }}
          onKeyDown={(e) => {
            if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
              e.preventDefault()
              setIndex((i) => wrap(found.length, i, e.key === 'ArrowDown' ? 1 : -1))
            } else if (e.key === 'Enter') {
              e.preventDefault()
              run(found[index])
            }
          }}
          placeholder="Aller à un rapport, exporter, changer de vue…"
          className="h-12 flex-1 bg-transparent text-sm outline-none"
        />
      </div>
      <ul id="palette-list" role="listbox" className="max-h-80 overflow-auto p-1">
        {found.length === 0 && <li className="px-3 py-6 text-center text-sm text-muted-foreground">Aucune commande.</li>}
        {found.map((c, i) => (
          <li
            key={c.id}
            id={`cmd-${c.id}`}
            role="option"
            aria-selected={i === index}
            onMouseMove={() => setIndex(i)}
            onClick={() => run(c)}
            className={cn('flex cursor-pointer items-center justify-between gap-3 rounded-lg px-3 py-2 text-sm', i === index && 'bg-accent')}
          >
            <span className="truncate">{c.label}</span>
            {c.hint && <span className="shrink-0 text-xs text-muted-foreground">{c.hint}</span>}
          </li>
        ))}
      </ul>
    </dialog>
  )
}
