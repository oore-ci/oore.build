import { Link } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import { ArrowDown01Icon } from '@hugeicons/core-free-icons'
import type {
  BuildTestFailure,
  BuildTestResultsResponse,
} from '@oore/client/models'

import { Alert, AlertDescription } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible'
import { Skeleton } from '@/components/ui/skeleton'
import { formatDuration } from '@/lib/format-utils'
import { cn } from '@/lib/utils'

function formatTestDuration(ms: number) {
  return ms < 1000 ? `${ms}ms` : formatDuration(ms / 1000)
}

function TestHistory({
  failure,
  buildId,
}: {
  failure: BuildTestFailure
  buildId: string
}) {
  const failedRuns = failure.history.filter(
    (run) => run.status === 'failed',
  ).length
  return (
    <div className="flex flex-wrap items-center gap-2">
      <ol
        className="flex items-center gap-0.5"
        aria-label={`Failed in ${failedRuns} of the last ${failure.history.length} builds that ran tests`}
      >
        {failure.history.map((run) => {
          const label = `Build #${run.build_number}: ${run.status}`
          // Shape as well as colour: failed runs are squares, passes are dots.
          const square = cn(
            'block size-3',
            run.status === 'failed'
              ? 'rounded-[2px] bg-destructive'
              : 'scale-75 rounded-full bg-success/70',
            run.build_id === buildId &&
              'ring-2 ring-ring ring-offset-1 ring-offset-background',
          )
          return (
            <li key={run.build_id}>
              {run.build_id === buildId ? (
                <span className={square} title={label} aria-label={label} />
              ) : (
                <Link
                  to="/builds/$buildId"
                  params={{ buildId: run.build_id }}
                  search={{ tab: 'tests' }}
                  className={cn(
                    square,
                    'outline-none hover:opacity-80 focus-visible:ring-[3px] focus-visible:ring-ring/50',
                  )}
                  title={label}
                  aria-label={label}
                />
              )}
            </li>
          )
        })}
      </ol>
      <span className="text-xs text-muted-foreground">
        Failed {failedRuns} of last {failure.history.length}
      </span>
    </div>
  )
}

function FailureRow({
  failure,
  buildId,
}: {
  failure: BuildTestFailure
  buildId: string
}) {
  return (
    <li className="space-y-2 py-4 first:pt-0 last:pb-0">
      <div className="flex flex-wrap items-start justify-between gap-2">
        <div className="min-w-0 space-y-0.5">
          <p className="text-sm font-medium break-words">{failure.name}</p>
          <p className="font-mono text-xs break-words text-muted-foreground">
            {failure.suite}
            {failure.duration_ms != null
              ? ` · ${formatTestDuration(failure.duration_ms)}`
              : null}
          </p>
        </div>
        {failure.flaky ? <Badge variant="warning">Flaky</Badge> : null}
      </div>
      {failure.message ? (
        <pre className="max-h-48 overflow-auto rounded-lg bg-muted/60 p-3 font-mono text-xs whitespace-pre-wrap text-foreground">
          {failure.message}
        </pre>
      ) : null}
      {failure.stack_trace ? (
        <Collapsible>
          <CollapsibleTrigger className="flex items-center gap-1 text-xs text-muted-foreground outline-none hover:text-foreground focus-visible:ring-[3px] focus-visible:ring-ring/50">
            Stack trace
            <HugeiconsIcon
              icon={ArrowDown01Icon}
              className="size-3.5 transition-transform in-data-open:rotate-180"
              aria-hidden
            />
          </CollapsibleTrigger>
          <CollapsibleContent>
            <pre className="mt-2 max-h-64 overflow-auto rounded-lg bg-muted/60 p-3 font-mono text-xs whitespace-pre-wrap text-muted-foreground">
              {failure.stack_trace}
            </pre>
          </CollapsibleContent>
        </Collapsible>
      ) : null}
      {failure.history.length > 1 ? (
        <TestHistory failure={failure} buildId={buildId} />
      ) : null}
    </li>
  )
}

export function TestsPanel({
  buildId,
  data,
  error,
  isLoading,
  onRetry,
}: {
  buildId: string
  data: BuildTestResultsResponse | undefined
  error: Error | null
  isLoading: boolean
  onRetry: () => void
}) {
  if (isLoading) return <Skeleton className="h-48 w-full" />
  if (error) {
    return (
      <Alert>
        <AlertDescription className="flex flex-wrap items-center justify-between gap-3">
          Could not load test results.
          <Button variant="outline" size="sm" onClick={onRetry}>
            Retry
          </Button>
        </AlertDescription>
      </Alert>
    )
  }
  const summary = data?.summary
  if (!summary) {
    return (
      <p className="text-sm text-muted-foreground">
        This build did not report test results.
      </p>
    )
  }
  const failures = data.failures
  const hiddenFailures = summary.failed - failures.length

  return (
    <Card size="sm">
      <CardHeader className="flex flex-wrap items-center justify-between gap-2">
        <CardTitle>
          {summary.failed > 0
            ? `${summary.failed} of ${summary.total} tests failed`
            : `All ${summary.total} tests passed`}
        </CardTitle>
        <div className="flex flex-wrap items-center gap-1.5">
          <Badge variant="success">{summary.passed} passed</Badge>
          {summary.failed > 0 ? (
            <Badge variant="destructive">{summary.failed} failed</Badge>
          ) : null}
          {summary.skipped > 0 ? (
            <Badge variant="outline">{summary.skipped} skipped</Badge>
          ) : null}
          <span className="text-xs text-muted-foreground">
            {formatTestDuration(summary.duration_ms)}
          </span>
        </div>
      </CardHeader>
      {failures.length > 0 ? (
        <CardContent>
          <ul className="divide-y">
            {failures.map((failure) => (
              <FailureRow
                key={`${failure.suite}\u001f${failure.name}`}
                failure={failure}
                buildId={buildId}
              />
            ))}
          </ul>
          {hiddenFailures > 0 ? (
            <p className="mt-4 text-xs text-muted-foreground">
              {hiddenFailures === 1
                ? '1 more failed test is not shown.'
                : `${hiddenFailures} more failed tests are not shown.`}{' '}
              Check the logs for the rest.
            </p>
          ) : null}
        </CardContent>
      ) : null}
    </Card>
  )
}
