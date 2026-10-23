import { HugeiconsIcon } from '@hugeicons/react'
import {
  AlertCircleIcon,
  InformationCircleIcon,
} from '@hugeicons/core-free-icons'

import type { FailureLine } from '@/components/terminal-log-viewer/log-model'
import type { BuildStateSummary } from '@/lib/build-state-description'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'

export function BuildStateAlert({
  summary,
  failureLine,
  nextStep,
  details,
  onShowFailureLine,
}: {
  summary: BuildStateSummary
  failureLine: FailureLine | null
  nextStep?: React.ReactNode
  /** Facts the reader needs to act, such as who asked for an approval. */
  details?: Array<{ label: string; value: React.ReactNode }>
  onShowFailureLine: () => void
}) {
  const destructive = summary.tone === 'destructive'
  return (
    <Alert variant={destructive ? 'destructive' : 'default'}>
      <HugeiconsIcon
        icon={destructive ? AlertCircleIcon : InformationCircleIcon}
        size={16}
      />
      <AlertTitle role="status">{summary.title}</AlertTitle>
      <AlertDescription className="flex min-w-0 flex-col gap-2">
        {failureLine ? (
          <>
            {/* oxlint-disable-next-line layout/max-class-utilities -- failure line chip */}
            <div className="flex min-w-0 items-start gap-2 rounded-lg border bg-muted/40 py-1 pr-1 pl-2.5">
              <code className="min-w-0 flex-1 py-0.5 font-mono text-xs wrap-break-word whitespace-pre-wrap text-foreground">
                {failureLine.text}
              </code>
              <Button
                variant="ghost"
                size="xs"
                className="shrink-0"
                aria-label={`Show line ${failureLine.lineNumber} in Logs`}
                onClick={onShowFailureLine}
              >
                Line {failureLine.lineNumber}
              </Button>
            </div>
            {nextStep ? <div>{nextStep}</div> : null}
          </>
        ) : summary.description || nextStep ? (
          <div>
            {summary.description} {nextStep}
          </div>
        ) : null}
        {details?.length ? (
          <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-1 text-sm sm:grid-cols-[repeat(2,auto_minmax(0,1fr))]">
            {details.map((detail) => (
              <div key={detail.label} className="contents">
                <dt className="text-muted-foreground">{detail.label}</dt>
                <dd className="min-w-0 truncate text-foreground">
                  {detail.value}
                </dd>
              </div>
            ))}
          </dl>
        ) : null}
      </AlertDescription>
    </Alert>
  )
}
