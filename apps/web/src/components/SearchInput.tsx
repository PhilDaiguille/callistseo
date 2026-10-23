import { useEffect, useState } from 'react'
import { Input } from '@/components/ui/input'

interface Props {
  value: string
  onChange: (value: string) => void
  placeholder: string
  className?: string
}

/** Champ de recherche : la valeur n'est poussée dans l'URL qu'après 300 ms sans frappe. */
export function SearchInput({ value, onChange, placeholder, className }: Props) {
  const [text, setText] = useState(value)
  // L'URL a changé ailleurs (bouton retour, réinitialisation) : on réaligne le champ.
  const [seen, setSeen] = useState(value)
  if (value !== seen) {
    setSeen(value)
    setText(value)
  }

  useEffect(() => {
    if (text === value) return
    const timer = setTimeout(() => onChange(text), 300)
    return () => clearTimeout(timer)
  }, [text, value, onChange])

  return (
    <Input
      type="search"
      value={text}
      onChange={(e) => setText(e.target.value)}
      placeholder={placeholder}
      aria-label={placeholder}
      className={className ?? 'max-w-sm'}
    />
  )
}
