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
  onShowFailureLine,
}: {
  summary: BuildStateSummary
  failureLine: FailureLine | null
  nextStep?: React.ReactNode
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
            <div className="flex min-w-0 items-start gap-2 rounded-lg border bg-muted/40 py-1 pr-1 pl-2.5">
              <code className="min-w-0 flex-1 py-0.5 font-mono text-xs break-words whitespace-pre-wrap text-foreground">
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
      </AlertDescription>
    </Alert>
  )
}
