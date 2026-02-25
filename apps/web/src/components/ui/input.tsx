import type { JSX } from 'solid-js'
import { cn } from '@/lib/utils'

interface InputProps extends JSX.InputHTMLAttributes<HTMLInputElement> {
  class?: string
}

export function Input(props: InputProps) {
  const { class: className, type, ...rest } = props
  return (
    <input
      type={type}
      data-slot="input"
      class={cn(
        'border-input placeholder:text-muted-foreground focus-visible:border-ring focus-visible:ring-ring/50 aria-invalid:border-destructive aria-invalid:ring-destructive/20 dark:aria-invalid:border-destructive/50 dark:aria-invalid:ring-destructive/40 h-9 w-full min-w-0 rounded-md border bg-transparent px-2.5 py-1 text-base shadow-xs outline-none transition-[color,box-shadow] file:inline-flex file:h-7 file:border-0 file:bg-transparent file:text-sm file:font-medium disabled:cursor-not-allowed disabled:pointer-events-none disabled:opacity-50 md:text-sm',
        className,
      )}
      {...rest}
    />
  )
}
