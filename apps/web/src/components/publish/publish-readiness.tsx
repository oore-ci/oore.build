import { useState, type ReactNode } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  AlertCircleIcon,
  ArrowDown01Icon,
  CheckmarkCircle02Icon,
  DashedLineCircleIcon,
} from '@hugeicons/core-free-icons'

import { cn } from '@/lib/utils'
import { Button } from '@/components/ui/button'
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible'
import { Stack } from '@/components/layout/stack'
import { Text } from '@/components/layout/text'

export interface ReadinessCheck {
  key: string
  label: string
  /** `null` when the check has not run yet; it does not count as missing. */
  ready: boolean | null
  detail: ReactNode
  /** A link to where the missing piece is set up. */
  action?: ReactNode
}

function CheckList({ checks }: { checks: Array<ReadinessCheck> }) {
  return (
    <ul className="flex flex-col gap-3">
      {checks.map((check) => (
        <li key={check.key} className="flex items-start gap-3 text-sm">
          <HugeiconsIcon
            icon={
              check.ready === null
                ? DashedLineCircleIcon
                : check.ready
                  ? CheckmarkCircle02Icon
                  : AlertCircleIcon
            }
            className={cn(
              'mt-0.5 size-4 shrink-0',
              check.ready === null
                ? 'text-muted-foreground'
                : check.ready
                  ? 'text-success'
                  : 'text-warning',
            )}
            aria-hidden
          />
          <Stack gap={0.5} className="min-w-0 flex-1">
            <span className="font-medium">
              {check.label}
              <span className="sr-only">
                {check.ready === null
                  ? ' (not checked)'
                  : check.ready
                    ? ' (done)'
                    : ' (needs attention)'}
              </span>
            </span>
            <span className="text-muted-foreground">{check.detail}</span>
          </Stack>
          {check.action ? <div className="shrink-0">{check.action}</div> : null}
        </li>
      ))}
    </ul>
  )
}

function ShowAllToggle({ count, open }: { count: number; open: boolean }) {
  return (
    <CollapsibleTrigger
      render={
        <Button
          type="button"
          variant="ghost"
          size="sm"
          className="-my-1 -mr-2 shrink-0 text-muted-foreground"
        />
      }
    >
      {open ? 'Hide checks' : `${count} ${count === 1 ? 'check' : 'checks'}`}
      <HugeiconsIcon
        icon={ArrowDown01Icon}
        data-icon="inline-end"
        className={cn('transition-transform', open && 'rotate-180')}
      />
    </CollapsibleTrigger>
  )
}

/**
 * Everything a store upload depends on, at the top of the store's
 * publishing section. When every check passes it is one line that opens
 * to the list; otherwise it lists only what still needs doing, each with a
 * link to the screen that fixes it.
 */
export function PublishReadiness({
  checkedAt,
  checks,
  footnote,
}: {
  /** When the store last confirmed the setup, as relative text. */
  checkedAt?: string
  checks: Array<ReadinessCheck>
  footnote?: ReactNode
}) {
  const [open, setOpen] = useState(false)
  const pending = checks.filter((check) => check.ready !== true)
  const missing = pending.filter((check) => check.ready === false).length
  const passed = checks.length - pending.length

  if (pending.length === 0) {
    return (
      <Collapsible
        open={open}
        onOpenChange={setOpen}
        role="group"
        aria-label="Publishing readiness"
        className="rounded-lg border bg-card px-4 py-3 sm:px-5"
      >
        <div className="flex items-center gap-3 text-sm">
          <HugeiconsIcon
            icon={CheckmarkCircle02Icon}
            className="size-4 shrink-0 text-success"
            aria-hidden
          />
          <p className="min-w-0 flex-1">
            <span className="font-medium">Ready to upload</span>
            {checkedAt ? (
              <span className="text-muted-foreground">
                , checked {checkedAt}
              </span>
            ) : null}
          </p>
          <ShowAllToggle count={checks.length} open={open} />
        </div>
        <CollapsibleContent>
          <div className="pt-4 pb-1">
            <CheckList checks={checks} />
          </div>
        </CollapsibleContent>
      </Collapsible>
    )
  }

  return (
    <Collapsible
      open={open}
      onOpenChange={setOpen}
      role="group"
      aria-label="Publishing readiness"
      className="flex flex-col gap-3 rounded-lg border bg-card p-4 sm:p-5"
    >
      <div className="flex items-center gap-3">
        <p className="min-w-0 flex-1 text-sm font-medium">
          {missing === 0
            ? 'Not checked yet'
            : `${missing} ${missing === 1 ? 'step' : 'steps'} before uploads can work`}
        </p>
        {passed > 0 ? (
          <ShowAllToggle count={checks.length} open={open} />
        ) : null}
      </div>
      <CheckList checks={open ? checks : pending} />
      {footnote ? <Text variant="muted">{footnote}</Text> : null}
    </Collapsible>
  )
}
