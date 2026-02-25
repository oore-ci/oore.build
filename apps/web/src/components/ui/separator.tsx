import type { JSX } from 'solid-js'
import { cn } from '@/lib/utils'

interface SeparatorProps extends JSX.HTMLAttributes<HTMLDivElement> {
  class?: string
  orientation?: 'horizontal' | 'vertical'
}

export function Separator(props: SeparatorProps) {
  const orientation = () => props.orientation ?? 'horizontal'
  const { class: className, orientation: _orientation, ...rest } = props

  return (
    <div
      role="separator"
      aria-orientation={orientation()}
      class={cn(
        'shrink-0 bg-border',
        orientation() === 'horizontal' ? 'h-px w-full' : 'h-full w-px',
        className,
      )}
      {...rest}
    />
  )
}
