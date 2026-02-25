import type { JSX } from 'solid-js'
import { cn } from '@/lib/utils'

interface SkeletonProps extends JSX.HTMLAttributes<HTMLDivElement> {
  class?: string
}

export function Skeleton(props: SkeletonProps) {
  const { class: className, ...rest } = props
  return (
    <div
      data-slot="skeleton"
      class={cn('bg-muted animate-pulse rounded-md', className)}
      {...rest}
    />
  )
}
