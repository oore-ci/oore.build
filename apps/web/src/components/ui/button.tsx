import { cva, type VariantProps } from 'class-variance-authority'
import type { JSX } from 'solid-js'
import { cn } from '@/lib/utils'

const buttonVariants = cva(
  'group/button inline-flex shrink-0 select-none items-center justify-center whitespace-nowrap border border-transparent bg-clip-padding text-sm font-medium outline-none transition-all disabled:pointer-events-none disabled:opacity-50',
  {
    variants: {
      variant: {
        default: 'bg-primary text-primary-foreground hover:bg-primary/80',
        outline:
          'border-border bg-background shadow-xs hover:bg-muted hover:text-foreground aria-expanded:bg-muted aria-expanded:text-foreground',
        secondary:
          'bg-secondary text-secondary-foreground hover:bg-secondary/80 aria-expanded:bg-secondary aria-expanded:text-secondary-foreground',
        ghost:
          'hover:bg-muted hover:text-foreground aria-expanded:bg-muted aria-expanded:text-foreground',
        destructive:
          'bg-destructive/10 text-destructive hover:bg-destructive/20 focus-visible:border-destructive/40',
        link: 'text-primary underline-offset-4 hover:underline',
      },
      size: {
        default: 'h-9 gap-1.5 px-2.5',
        xs: 'h-6 gap-1 rounded-[min(var(--radius-md),8px)] px-2 text-xs [&_svg:not([class*=size-])]:size-3',
        sm: 'h-8 gap-1 rounded-[min(var(--radius-md),10px)] px-2.5',
        lg: 'h-10 gap-1.5 px-2.5',
        icon: 'size-9',
        'icon-xs':
          'size-6 rounded-[min(var(--radius-md),8px)] [&_svg:not([class*=size-])]:size-3',
        'icon-sm': 'size-8 rounded-[min(var(--radius-md),10px)]',
        'icon-lg': 'size-10',
      },
    },
    defaultVariants: {
      variant: 'default',
      size: 'default',
    },
  },
)

interface ButtonProps
  extends JSX.ButtonHTMLAttributes<HTMLButtonElement>,
    VariantProps<typeof buttonVariants> {
  class?: string
}

export function Button(props: ButtonProps) {
  const { class: className, variant, size, ...rest } = props

  return (
    <button
      data-slot="button"
      class={cn(buttonVariants({ variant, size, className }))}
      {...rest}
    />
  )
}

export { buttonVariants }
