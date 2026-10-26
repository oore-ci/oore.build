import { Fragment, useId, useState } from 'react'
import { Link, type LinkProps } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import { ArrowDown01Icon, ChevronRightIcon } from '@hugeicons/core-free-icons'

import { Alert, AlertDescription } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { ItemGroup, ItemSeparator } from '@/components/ui/item'
import { useIsMobile } from '@/hooks/use-mobile'
import { cn } from '@/lib/utils'
import { Row } from '@/components/layout/row'
import { Stack } from '@/components/layout/stack'
import { textVariants } from '@/lib/recipes'

/**
 * One heading, count, scope and action treatment for every Home section.
 * `collapsedOnMobile` folds long reference sections behind a Show toggle on
 * phones so what needs attention stays near the top.
 */
export function DashboardSection({
  action,
  children,
  collapsedOnMobile = false,
  count,
  scope,
  title,
}: {
  action?: React.ReactNode
  children: React.ReactNode
  collapsedOnMobile?: boolean
  count?: number
  scope?: string
  title: string
}) {
  const titleId = useId()
  const contentId = useId()
  const isMobile = useIsMobile()
  const [expanded, setExpanded] = useState<boolean | null>(null)
  const collapsible = collapsedOnMobile && isMobile
  const open = !collapsible || (expanded ?? false)

  return (
    <section aria-labelledby={titleId} className="flex min-w-0 flex-col gap-3">
      <div className="flex min-h-7 flex-wrap items-center justify-between gap-x-3 gap-y-2">
        <Row gap={2} className="min-w-0">
          <h2 id={titleId} className="text-sm font-semibold">
            {title}
          </h2>
          {count == null ? null : <Badge variant="outline">{count}</Badge>}
          {scope ? (
            <span className={textVariants({ variant: 'muted' })}>{scope}</span>
          ) : null}
        </Row>
        <Row gap={2}>
          {open ? action : null}
          {collapsible ? (
            <Button
              variant="ghost"
              size="sm"
              className="-me-2"
              aria-expanded={open}
              aria-controls={contentId}
              onClick={() => setExpanded(!open)}
            >
              {open ? 'Hide' : 'Show'}
              <HugeiconsIcon
                icon={ArrowDown01Icon}
                data-icon="inline-end"
                className={cn('transition-transform', open && 'rotate-180')}
              />
            </Button>
          ) : null}
        </Row>
      </div>
      {open ? (
        <Stack id={contentId} gap={3} className="min-w-0">
          {children}
        </Stack>
      ) : null}
    </section>
  )
}

/** The section-level link to the full collection, flush with the section edge. */
export function DashboardViewAll(props: Pick<LinkProps, 'to' | 'search'>) {
  return (
    <Button
      variant="ghost"
      size="sm"
      className="-me-2"
      render={<Link {...props} />}
      nativeButton={false}
    >
      View all
      <HugeiconsIcon icon={ChevronRightIcon} data-icon="inline-end" />
    </Button>
  )
}

/** Related rows on one bordered surface, divided by separators. */
export function DashboardList({
  children,
}: {
  children: Array<{ key: string; node: React.ReactNode }>
}) {
  return (
    <div className="overflow-hidden rounded-xl border bg-card">
      <ItemGroup className="gap-0">
        {children.map(({ key, node }, index) => (
          <Fragment key={key}>
            {index > 0 ? <ItemSeparator className="my-0" /> : null}
            {node}
          </Fragment>
        ))}
      </ItemGroup>
    </div>
  )
}

/** A section's failed load, never shown as an empty section. */
export function DashboardSectionError({
  label,
  onRetry,
}: {
  label: string
  onRetry: () => void
}) {
  return (
    <Alert variant="destructive">
      <AlertDescription className="flex items-center justify-between gap-3">
        <span>{label} could not be loaded.</span>
        <Button variant="outline" size="sm" onClick={onRetry}>
          Retry
        </Button>
      </AlertDescription>
    </Alert>
  )
}
