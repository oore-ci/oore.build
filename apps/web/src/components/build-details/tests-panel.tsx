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
import { Row } from '@/components/layout/row'
import { Text } from '@/components/layout/text'
import {
  focusRingVariants,
  monoVariants,
  rowVariants,
  textVariants,
} from '@/lib/recipes'

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
    <Row gap={2} className="flex-wrap">
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
      <span className={textVariants({ variant: 'caption' })}>
        Failed {failedRuns} of last {failure.history.length}
      </span>
    </Row>
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
          <p className="text-sm font-medium wrap-break-word">{failure.name}</p>
          <Text variant="mono" className="wrap-break-word">
            {failure.suite}
            {failure.duration_ms != null
              ? ` · ${formatTestDuration(failure.duration_ms)}`
              : null}
          </Text>
        </div>
        {failure.flaky ? <Badge variant="warning">Flaky</Badge> : null}
      </div>
      {failure.message ? (
        <pre
          className={cn(
            monoVariants({ size: 'xs' }),
            'max-h-48 overflow-auto rounded-lg bg-muted/60 p-3 whitespace-pre-wrap text-foreground',
          )}
        >
          {failure.message}
        </pre>
      ) : null}
      {failure.stack_trace ? (
        <Collapsible>
          <CollapsibleTrigger
            className={cn(
              textVariants({ variant: 'caption' }),
              rowVariants({ gap: 1 }),
              focusRingVariants(),
              'hover:text-foreground',
            )}
          >
            Stack trace
            <HugeiconsIcon
              icon={ArrowDown01Icon}
              className="size-3.5 transition-transform in-data-open:rotate-180"
              aria-hidden
            />
          </CollapsibleTrigger>
          <CollapsibleContent>
            <pre
              className={cn(
                textVariants({ variant: 'mono' }),
                'mt-2 max-h-64 overflow-auto rounded-lg bg-muted/60 p-3 whitespace-pre-wrap',
              )}
            >
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
    return <Text variant="muted">This build did not report test results.</Text>
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
        <Row gap={1.5} className="flex-wrap">
          <Badge variant="success">{summary.passed} passed</Badge>
          {summary.failed > 0 ? (
            <Badge variant="destructive">{summary.failed} failed</Badge>
          ) : null}
          {summary.skipped > 0 ? (
            <Badge variant="outline">{summary.skipped} skipped</Badge>
          ) : null}
          <span className={textVariants({ variant: 'caption' })}>
            {formatTestDuration(summary.duration_ms)}
          </span>
        </Row>
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
            <Text variant="caption" className="mt-4">
              {hiddenFailures === 1
                ? '1 more failed test is not shown.'
                : `${hiddenFailures} more failed tests are not shown.`}{' '}
              Check the logs for the rest.
            </Text>
          ) : null}
        </CardContent>
      ) : null}
    </Card>
  )
}
