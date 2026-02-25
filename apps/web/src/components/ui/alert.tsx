import { cva, type VariantProps } from 'class-variance-authority'
import type { JSX } from 'solid-js'
import { cn } from '@/lib/utils'

const alertVariants = cva(
  'group/alert relative grid w-full gap-0.5 rounded-lg border px-4 py-3 text-left text-sm has-[>svg]:grid-cols-[auto_1fr] has-[>svg]:gap-x-2.5 has-data-[slot=alert-action]:relative has-data-[slot=alert-action]:pr-18 *:[svg]:row-span-2 *:[svg]:translate-y-0.5 *:[svg]:text-current *:[svg:not([class*=size-])]:size-4',
  {
  variants: {
    variant: {
      default: 'bg-card text-card-foreground',
      destructive:
        'bg-card text-destructive *:data-[slot=alert-description]:text-destructive/90 *:[svg]:text-current',
    },
  },
  defaultVariants: {
    variant: 'default',
  },
})

interface AlertProps
  extends JSX.HTMLAttributes<HTMLDivElement>,
    VariantProps<typeof alertVariants> {
  class?: string
}

export function Alert(props: AlertProps) {
  const { class: className, variant, ...rest } = props
  return (
    <div
      data-slot="alert"
      role="alert"
      class={cn(alertVariants({ variant, className }))}
      {...rest}
    />
  )
}

interface AlertTextProps extends JSX.HTMLAttributes<HTMLDivElement> {
  class?: string
}

export function AlertTitle(props: AlertTextProps) {
  const { class: className, ...rest } = props
  return (
    <div
      data-slot="alert-title"
      class={cn(
        'font-medium group-has-[>svg]/alert:col-start-2 [&_a]:underline [&_a]:underline-offset-3 [&_a]:hover:text-foreground',
        className,
      )}
      {...rest}
    />
  )
}

export function AlertDescription(props: AlertTextProps) {
  const { class: className, ...rest } = props
  return (
    <div
      data-slot="alert-description"
      class={cn(
        'text-muted-foreground text-sm text-balance md:text-pretty [&_a]:underline [&_a]:underline-offset-3 [&_a]:hover:text-foreground [&_p:not(:last-child)]:mb-4',
        className,
      )}
      {...rest}
    />
  )
}

export function AlertAction(props: AlertTextProps) {
  const { class: className, ...rest } = props
  return (
    <div
      data-slot="alert-action"
      class={cn('absolute top-2.5 right-3', className)}
      {...rest}
    />
  )
}
