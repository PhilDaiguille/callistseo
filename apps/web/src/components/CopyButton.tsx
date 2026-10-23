import { useEffect, useRef, useState } from 'react'
import { Check, Copy } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'

/** Copie `text` dans le presse-papiers ; l'icône confirme pendant deux secondes. */
export function CopyButton({ text, label = 'Copier', className, tabIndex }: { text: string; label?: string; className?: string; tabIndex?: number }) {
  const [copied, setCopied] = useState(false)
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)
  useEffect(() => () => clearTimeout(timer.current), [])
  return (
    <Button
      type="button"
      variant="ghost"
      size="icon-xs"
      className={cn('text-muted-foreground', className)}
      aria-label={copied ? 'Copié' : label}
      title={copied ? 'Copié' : label}
      tabIndex={tabIndex}
      onClick={(e) => {
        e.stopPropagation()
        void navigator.clipboard
          .writeText(text)
          .then(() => {
            setCopied(true)
            clearTimeout(timer.current)
            timer.current = setTimeout(() => setCopied(false), 2000)
          })
          .catch(() => {})
      }}
    >
      {copied ? <Check aria-hidden /> : <Copy aria-hidden />}
    </Button>
  )
}
