import type { JSX } from 'solid-js'
import { Link } from '@tanstack/solid-router'
import { ArrowLeft02Icon } from '@hugeicons/core-free-icons'
import { HugeIcon } from '@/components/huge-icon'
import { cn } from '@/lib/utils'

interface PageHeaderProps {
  title: string
  description?: string
  actions?: JSX.Element
  back?: { to: string; label: string }
  meta?: JSX.Element
  class?: string
}

export function PageHeader(props: PageHeaderProps) {
  return (
    <header class={cn('space-y-3 pb-6', props.class)}>
      {props.back ? (
        <Link
          to={props.back.to as never}
          class="inline-flex items-center gap-1.5 text-xs font-medium uppercase tracking-wider text-muted-foreground transition-colors hover:text-foreground"
        >
          <HugeIcon icon={ArrowLeft02Icon} size={14} />
          {props.back.label}
        </Link>
      ) : null}

      <div class="flex flex-col gap-4 lg:flex-row lg:items-end lg:justify-between">
        <div class="min-w-0 space-y-1">
          <h1 class="text-3xl font-bold tracking-tight">{props.title}</h1>
          {props.description ? (
            <p class="text-sm text-muted-foreground">{props.description}</p>
          ) : null}
        </div>

        {props.actions ? (
          <div class="flex flex-wrap items-center gap-2">{props.actions}</div>
        ) : null}
      </div>

      {props.meta ? (
        <div class="flex flex-wrap items-center gap-2 pt-2 text-xs text-muted-foreground">
          {props.meta}
        </div>
      ) : null}
    </header>
  )
}
