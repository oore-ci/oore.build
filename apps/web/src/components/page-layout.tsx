import type { JSX } from 'solid-js'
import { cn } from '@/lib/utils'

const WIDTH_CLASSES = {
  default: 'max-w-4xl',
  narrow: 'max-w-xl',
  wide: 'max-w-6xl',
  full: '',
} as const

interface PageLayoutProps extends JSX.HTMLAttributes<HTMLDivElement> {
  width?: keyof typeof WIDTH_CLASSES
  class?: string
}

export function PageLayout(props: PageLayoutProps) {
  const {
    class: className,
    width = 'default',
    children,
    ...rest
  } = props

  return (
    <div
      class={cn(
        WIDTH_CLASSES[width],
        'mx-auto w-full space-y-6 px-6 py-8 lg:px-10 lg:py-10',
        className,
      )}
      {...rest}
    >
      {children}
    </div>
  )
}
