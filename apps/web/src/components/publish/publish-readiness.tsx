import type { ReactNode } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  AlertCircleIcon,
  CheckmarkCircle02Icon,
  DashedLineCircleIcon,
} from '@hugeicons/core-free-icons'

import { cn } from '@/lib/utils'

export interface ReadinessCheck {
  key: string
  label: string
  /** `null` when the check has not run yet; it does not count as missing. */
  ready: boolean | null
  detail: ReactNode
  /** A link to where the missing piece is set up. */
  action?: ReactNode
}

/**
 * Everything a store upload depends on, in one list at the top of the
 * store's publishing section, with a link to the screen that fixes each gap.
 */
export function PublishReadiness({
  checks,
  footnote,
}: {
  checks: Array<ReadinessCheck>
  footnote?: ReactNode
}) {
  const missing = checks.filter((check) => check.ready === false).length
  return (
    <div
      role="group"
      aria-label="Publishing readiness"
      className="flex flex-col gap-3 rounded-lg border bg-card p-4 sm:p-5"
    >
      <p className="text-sm font-medium">
        {missing === 0
          ? 'Ready to upload'
          : `${missing} ${missing === 1 ? 'step' : 'steps'} before uploads can work`}
      </p>
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
            <div className="flex min-w-0 flex-1 flex-col gap-0.5">
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
            </div>
            {check.action ? (
              <div className="shrink-0">{check.action}</div>
            ) : null}
          </li>
        ))}
      </ul>
      {footnote ? (
        <p className="text-sm text-muted-foreground">{footnote}</p>
      ) : null}
    </div>
  )
}
