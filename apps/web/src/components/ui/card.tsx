import type { JSX } from 'solid-js'
import { cn } from '@/lib/utils'

interface DivProps extends JSX.HTMLAttributes<HTMLDivElement> {
  class?: string
}

interface CardProps extends DivProps {
  size?: 'default' | 'sm'
}

export function Card(props: CardProps) {
  const { class: className, size = 'default', ...rest } = props
  return (
    <div
      data-slot="card"
      data-size={size}
      class={cn(
        'ring-foreground/10 bg-card text-card-foreground group/card flex flex-col gap-6 overflow-hidden py-6 text-sm shadow-xs ring-1 has-[>img:first-child]:pt-0 data-[size=sm]:gap-4 data-[size=sm]:py-4 *:[img:first-child]:rounded-t-xl *:[img:last-child]:rounded-b-xl',
        className,
      )}
      {...rest}
    />
  )
}

export function CardHeader(props: DivProps) {
  const { class: className, ...rest } = props
  return (
    <div
      data-slot="card-header"
      class={cn(
        'group/card-header grid auto-rows-min items-start gap-1 rounded-t-xl px-6 [.border-b]:pb-6 group-data-[size=sm]/card:px-4 group-data-[size=sm]/card:[.border-b]:pb-4 @container/card-header has-data-[slot=card-action]:grid-cols-[1fr_auto] has-data-[slot=card-description]:grid-rows-[auto_auto]',
        className,
      )}
      {...rest}
    />
  )
}

export function CardTitle(props: DivProps) {
  const { class: className, ...rest } = props
  return (
    <div
      data-slot="card-title"
      class={cn(
        'text-base leading-normal font-medium group-data-[size=sm]/card:text-sm',
        className,
      )}
      {...rest}
    />
  )
}

export function CardDescription(props: DivProps) {
  const { class: className, ...rest } = props
  return (
    <div
      data-slot="card-description"
      class={cn('text-muted-foreground text-sm', className)}
      {...rest}
    />
  )
}

export function CardAction(props: DivProps) {
  const { class: className, ...rest } = props
  return (
    <div
      data-slot="card-action"
      class={cn(
        'col-start-2 row-span-2 row-start-1 self-start justify-self-end',
        className,
      )}
      {...rest}
    />
  )
}

export function CardContent(props: DivProps) {
  const { class: className, ...rest } = props
  return (
    <div
      data-slot="card-content"
      class={cn('px-6 group-data-[size=sm]/card:px-4', className)}
      {...rest}
    />
  )
}

export function CardFooter(props: DivProps) {
  const { class: className, ...rest } = props
  return (
    <div
      data-slot="card-footer"
      class={cn(
        'flex items-center rounded-b-xl px-6 [.border-t]:pt-6 group-data-[size=sm]/card:px-4 group-data-[size=sm]/card:[.border-t]:pt-4',
        className,
      )}
      {...rest}
    />
  )
}
