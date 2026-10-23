import { Button as ButtonPrimitive } from "@base-ui/react/button"
import { cva, type VariantProps } from "class-variance-authority"
import { cn } from "@/lib/utils"

// Niveau FORT : contour encre 2 px + ombre décalée (sans flou). Au survol le bouton se soulève,
// à l'appui il « s'enfonce » (l'ombre disparaît). `--pop` règle la taille de l'ombre.
const pressable =
  "border-ink shadow-[var(--pop)_var(--pop)_0_0_var(--shadow-color)] hover:-translate-x-px hover:-translate-y-px hover:shadow-[calc(var(--pop)+1px)_calc(var(--pop)+1px)_0_0_var(--shadow-color)] active:translate-x-px active:translate-y-px active:shadow-none motion-reduce:transition-none"

const buttonVariants = cva(
  "group/button inline-flex shrink-0 items-center justify-center rounded-xl border-2 border-transparent text-sm font-semibold whitespace-nowrap transition-[transform,box-shadow,background-color] duration-100 outline-none select-none [--pop:3px] disabled:pointer-events-none disabled:opacity-50 aria-invalid:border-destructive [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4",
  {
    variants: {
      variant: {
        // L'action principale de l'écran (une seule par écran).
        default: `bg-primary text-primary-foreground ${pressable}`,
        secondary: `bg-card text-foreground ${pressable}`,
        destructive: `bg-destructive-soft text-destructive ${pressable}`,
        // À plat : barres d'outils, actions répétées dans les tableaux.
        outline: "border-line bg-card text-foreground hover:bg-accent aria-expanded:bg-accent",
        ghost: "text-foreground hover:bg-accent aria-expanded:bg-accent",
        link: "text-foreground underline underline-offset-4 hover:no-underline",
      },
      size: {
        default: "h-10 gap-2 px-4",
        xs: "h-7 gap-1 rounded-lg px-2.5 text-xs [--pop:2px] [&_svg:not([class*='size-'])]:size-3",
        sm: "h-8 gap-1.5 rounded-lg px-3 text-[0.8rem] [--pop:2px] [&_svg:not([class*='size-'])]:size-3.5",
        lg: "h-11 gap-2 px-5 text-base",
        icon: "size-10",
        "icon-xs": "size-7 rounded-lg [--pop:2px] [&_svg:not([class*='size-'])]:size-3",
        "icon-sm": "size-8 rounded-lg [--pop:2px]",
        "icon-lg": "size-11",
      },
    },
    defaultVariants: {
      variant: "default",
      size: "default",
    },
  }
)

function Button({
  className,
  variant = "default",
  size = "default",
  ...props
}: ButtonPrimitive.Props & VariantProps<typeof buttonVariants>) {
  return (
    <ButtonPrimitive
      data-slot="button"
      className={cn(buttonVariants({ variant, size, className }))}
      {...props}
    />
  )
}

export { Button, buttonVariants }
