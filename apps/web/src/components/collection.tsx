import type { ReactNode } from 'react'
import { InformationCircleIcon } from '@hugeicons/core-free-icons'
import { HugeiconsIcon } from '@hugeicons/react'

import { Alert, AlertAction, AlertDescription, AlertTitle } from './ui/alert'
import { Button } from './ui/button'
import { cn } from '@/lib/utils'

export function CollectionFrame({
  ariaLabel,
  children,
  className,
  isBusy = false,
}: {
  ariaLabel: string
  children: ReactNode
  className?: string
  isBusy?: boolean
}) {
  return (
    <section
      aria-label={ariaLabel}
      aria-busy={isBusy}
      data-slot="collection"
      // A true-empty state sits at its content height instead of stretching
      // a dashed frame down the whole page.
      className={cn(
        'flex min-h-0 min-w-0 flex-1 flex-col gap-3 *:data-[slot=empty]:flex-none',
        className,
      )}
    >
      {children}
    </section>
  )
}

export function CollectionError({
  description,
  onRetry,
  title,
}: {
  description?: string
  onRetry: () => void
  title: string
}) {
  return (
    <Alert variant="destructive">
      <HugeiconsIcon icon={InformationCircleIcon} />
      <AlertTitle>{title}</AlertTitle>
      {description ? <AlertDescription>{description}</AlertDescription> : null}
      <AlertAction>
        <Button variant="outline" size="sm" onClick={onRetry}>
          Retry
        </Button>
      </AlertAction>
    </Alert>
  )
}
