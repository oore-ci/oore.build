import { Link } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  ArrowDown01Icon,
  ArrowUp01Icon,
  ChevronRightIcon,
  CheckmarkCircle02Icon as CheckCircleIcon,
} from '@hugeicons/core-free-icons'

import type {
  BuildInsightsDay,
  BuildInsightsResponse,
  FailingPipelineInsights,
} from '@oore/client/models'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from '@/components/ui/card'
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from '@/components/ui/empty'
import {
  Item,
  ItemActions,
  ItemContent,
  ItemDescription,
  ItemGroup,
  ItemTitle,
} from '@/components/ui/item'
import { Skeleton } from '@/components/ui/skeleton'
import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group'
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from '@/components/ui/tooltip'
import { useBuildInsights, type BuildInsightsWindow } from '@/hooks/use-builds'
import { formatDuration, relativeTime } from '@/lib/format-utils'
import { cn } from '@/lib/utils'

const WINDOWS: Array<BuildInsightsWindow> = [7, 30, 90]

function formatRate(rate?: number | null): string {
  return rate == null ? '—' : `${Math.round(rate * 100)}%`
}

function formatSeconds(seconds?: number | null): string {
  return seconds == null ? '—' : formatDuration(seconds)
}

/**
 * A change against the previous window. `goodWhen` says which direction is an
 * improvement, so a faster build time reads as good.
 */
function Delta({
  current,
  previous,
  format,
  goodWhen,
  label,
}: {
  current?: number | null
  previous?: number | null
  format: (difference: number) => string
  goodWhen: 'up' | 'down'
  label: string
}) {
  if (current == null || previous == null) {
    return <Badge variant="outline">No earlier data</Badge>
  }
  const difference = current - previous
  const text = format(Math.abs(difference))
  if (text === format(0)) return <Badge variant="outline">No change</Badge>
  const up = difference > 0
  const good = up === (goodWhen === 'up')
  return (
    <Badge
      variant={good ? 'success' : 'destructive'}
      aria-label={`${up ? 'Up' : 'Down'} ${text} ${label}`}
    >
      <HugeiconsIcon
        icon={up ? ArrowUp01Icon : ArrowDown01Icon}
        data-icon="inline-start"
      />
      {text}
    </Badge>
  )
}

function MetricCard({
  title,
  value,
  badge,
  footer,
  to,
  search,
  label,
}: {
  title: string
  value: string
  badge: React.ReactNode
  footer: string
  to: '/builds' | '/settings/runners'
  search?: { status: string }
  label: string
}) {
  return (
    <Link
      to={to}
      search={search}
      aria-label={label}
      className="rounded-xl outline-none focus-visible:ring-[3px] focus-visible:ring-ring/50"
    >
      <Card size="sm" className="@container/card h-full">
        <CardHeader>
          <CardDescription>{title}</CardDescription>
          <CardTitle className="font-mono text-2xl font-semibold tabular-nums @[250px]/card:text-3xl">
            {value}
          </CardTitle>
          <CardAction>{badge}</CardAction>
        </CardHeader>
        <CardFooter className="justify-between gap-2 text-muted-foreground">
          <span className="line-clamp-2">{footer}</span>
          <HugeiconsIcon icon={ChevronRightIcon} className="shrink-0" />
        </CardFooter>
      </Card>
    </Link>
  )
}

function BuildMetrics({
  insights,
  onlineRunners,
  totalRunners,
}: {
  insights: BuildInsightsResponse
  onlineRunners?: number
  totalRunners?: number
}) {
  const { totals, previous_totals: previous, queue } = insights
  const failures = totals.failed + totals.timed_out
  const verdicts = totals.succeeded + failures
  const window = `${insights.window_days} days`
  const runners =
    onlineRunners === undefined || totalRunners === undefined
      ? null
      : `${onlineRunners} of ${totalRunners} runners online`
  const waitText =
    queue.oldest_waiting_since != null
      ? `Oldest queued ${relativeTime(queue.oldest_waiting_since)}`
      : insights.queue_wait.p50_seconds != null
        ? `Median wait ${formatDuration(insights.queue_wait.p50_seconds)}`
        : 'No builds waited'

  return (
    <div className="grid grid-cols-1 gap-3 @xl/main:grid-cols-2 @5xl/main:grid-cols-4">
      <MetricCard
        title="Success rate"
        value={formatRate(totals.success_rate)}
        badge={
          <Delta
            current={totals.success_rate}
            previous={previous.success_rate}
            format={(difference) => `${Math.round(difference * 100)} pts`}
            goodWhen="up"
            label="against the previous period"
          />
        }
        footer={
          verdicts === 0
            ? `No finished builds in ${window}`
            : `${totals.succeeded} of ${verdicts} builds passed`
        }
        to="/builds"
        search={failures > 0 ? { status: 'failed' } : undefined}
        label={`Success rate ${formatRate(totals.success_rate)} over ${window}. Open failed builds`}
      />
      <MetricCard
        title="Builds finished"
        value={String(totals.finished)}
        badge={
          <Delta
            current={totals.finished}
            previous={previous.finished}
            format={(difference) => String(difference)}
            goodWhen="up"
            label="builds against the previous period"
          />
        }
        footer={
          failures + totals.canceled + totals.expired === 0
            ? `Over the last ${window}`
            : [
                failures > 0 ? `${failures} failed` : null,
                totals.canceled > 0 ? `${totals.canceled} canceled` : null,
                totals.expired > 0 ? `${totals.expired} expired` : null,
              ]
                .filter(Boolean)
                .join(' · ')
        }
        to="/builds"
        label={`${totals.finished} builds finished over ${window}. Open builds`}
      />
      <MetricCard
        title="Median build time"
        value={formatSeconds(insights.duration.p50_seconds)}
        badge={
          <Delta
            current={insights.duration.p50_seconds}
            previous={insights.previous_duration.p50_seconds}
            format={(difference) => formatDuration(difference)}
            goodWhen="down"
            label="median build time against the previous period"
          />
        }
        footer={
          insights.duration.p90_seconds == null
            ? 'From succeeded builds'
            : `p90 ${formatDuration(insights.duration.p90_seconds)} · succeeded builds`
        }
        to="/builds"
        search={{ status: 'succeeded' }}
        label={`Median build time ${formatSeconds(insights.duration.p50_seconds)}. Open succeeded builds`}
      />
      <MetricCard
        title="Waiting now"
        value={String(queue.waiting)}
        badge={<Badge variant="outline">{queue.running} running</Badge>}
        footer={[
          waitText,
          queue.awaiting_approval > 0
            ? `${queue.awaiting_approval} to approve`
            : null,
          runners,
        ]
          .filter(Boolean)
          .join(' · ')}
        to={
          queue.waiting > 0 && onlineRunners === 0
            ? '/settings/runners'
            : '/builds'
        }
        search={
          queue.waiting > 0 && onlineRunners !== 0
            ? { status: 'queued' }
            : undefined
        }
        label={`${queue.waiting} builds waiting and ${queue.running} running. ${waitText}`}
      />
    </div>
  )
}

const DAY_LABEL = new Intl.DateTimeFormat(undefined, {
  month: 'short',
  day: 'numeric',
})

function dayLabel(day: BuildInsightsDay): string {
  return DAY_LABEL.format(new Date(day.day_start * 1000))
}

function daySummary(day: BuildInsightsDay): string {
  const parts = [`${day.succeeded} passed`, `${day.failed} failed`]
  if (day.other > 0) parts.push(`${day.other} canceled or expired`)
  if (day.p50_duration_seconds != null) {
    parts.push(`median ${formatDuration(day.p50_duration_seconds)}`)
  }
  return parts.join(', ')
}

const SERIES = [
  { key: 'succeeded', label: 'Passed', className: 'bg-success' },
  { key: 'failed', label: 'Failed', className: 'bg-destructive' },
  { key: 'other', label: 'Canceled', className: 'bg-muted-foreground/40' },
] as const

/** Finished builds per day, stacked by outcome. */
function BuildTrend({ insights }: { insights: BuildInsightsResponse }) {
  const days = insights.daily
  const max = Math.max(
    1,
    ...days.map((day) => day.succeeded + day.failed + day.other),
  )
  const middle = days[Math.floor(days.length / 2)]

  return (
    <Card size="sm" className="h-full">
      <CardHeader>
        <CardTitle>Builds per day</CardTitle>
        <CardDescription>
          <ul className="flex flex-wrap gap-x-3 gap-y-1">
            {SERIES.map((series) => (
              <li key={series.key} className="flex items-center gap-1.5">
                <span
                  className={cn('size-2 rounded-full', series.className)}
                  aria-hidden
                />
                {series.label}
              </li>
            ))}
          </ul>
        </CardDescription>
      </CardHeader>
      <CardContent className="flex flex-1 flex-col gap-2">
        <div className="relative flex min-h-40 flex-1 gap-2">
          <div
            className="flex flex-col justify-between font-mono text-xs text-muted-foreground tabular-nums"
            aria-hidden
          >
            <span>{max}</span>
            <span>0</span>
          </div>
          <div
            className={cn(
              'flex flex-1 items-end border-b border-border',
              days.length > 31 ? 'gap-px' : 'gap-1',
            )}
            role="list"
            aria-label="Finished builds per day"
          >
            {days.map((day) => {
              const total = day.succeeded + day.failed + day.other
              return (
                <Tooltip key={day.day_start}>
                  <TooltipTrigger
                    render={
                      <div
                        role="listitem"
                        tabIndex={0}
                        aria-label={`${dayLabel(day)}: ${daySummary(day)}`}
                        className="group flex h-full min-w-0 flex-1 flex-col items-center justify-end rounded-sm outline-none focus-visible:ring-[3px] focus-visible:ring-ring/50"
                      />
                    }
                  >
                    {total === 0 ? (
                      <span className="h-px w-full max-w-6 bg-border" />
                    ) : (
                      <span
                        className="flex w-full max-w-6 flex-col-reverse gap-0.5 group-hover:opacity-80"
                        style={{ height: `${(total / max) * 100}%` }}
                      >
                        {SERIES.map((series) =>
                          day[series.key] > 0 ? (
                            <span
                              key={series.key}
                              className={cn(
                                'min-h-0.5 w-full last:rounded-t-sm',
                                series.className,
                              )}
                              style={{ flexGrow: day[series.key] }}
                            />
                          ) : null,
                        )}
                      </span>
                    )}
                  </TooltipTrigger>
                  <TooltipContent>
                    <span className="flex flex-col gap-0.5">
                      <span className="font-medium">{dayLabel(day)}</span>
                      <span>{daySummary(day)}</span>
                    </span>
                  </TooltipContent>
                </Tooltip>
              )
            })}
          </div>
        </div>
        <div
          className="flex justify-between pl-6 text-xs text-muted-foreground"
          aria-hidden
        >
          <span>{days[0] ? dayLabel(days[0]) : null}</span>
          {days.length > 2 && middle ? <span>{dayLabel(middle)}</span> : null}
          <span>Today</span>
        </div>
      </CardContent>
    </Card>
  )
}

function failingDescription(pipeline: FailingPipelineInsights): string {
  const streak =
    pipeline.current_streak > 1
      ? `Failed ${pipeline.current_streak} in a row`
      : pipeline.current_streak === 1
        ? 'Last build failed'
        : 'Passing again'
  return `${streak} · ${pipeline.failures} of ${pipeline.finished} failed · ${relativeTime(pipeline.last_failed_at)}`
}

function FailingPipelines({
  pipelines,
}: {
  pipelines: Array<FailingPipelineInsights>
}) {
  return (
    <Card size="sm" className="h-full">
      <CardHeader>
        <CardTitle>Failing pipelines</CardTitle>
        <CardDescription>
          Most failures first, broken ones on top
        </CardDescription>
      </CardHeader>
      <CardContent className="flex-1">
        {pipelines.length === 0 ? (
          <Empty className="h-full p-4">
            <EmptyHeader>
              <EmptyMedia variant="icon">
                <HugeiconsIcon icon={CheckCircleIcon} />
              </EmptyMedia>
              <EmptyTitle>No failures</EmptyTitle>
              <EmptyDescription>
                Every finished build in this period passed.
              </EmptyDescription>
            </EmptyHeader>
          </Empty>
        ) : (
          <ItemGroup className="gap-1">
            {pipelines.map((pipeline) => (
              <Item
                key={pipeline.pipeline_id}
                size="sm"
                className="-mx-3"
                render={
                  <Link
                    to="/builds/$buildId"
                    params={{ buildId: pipeline.last_failed_build_id }}
                    aria-label={`Open ${pipeline.project_name} build #${pipeline.last_failed_build_number}, the last failure of ${pipeline.pipeline_name}`}
                  />
                }
              >
                <ItemContent className="min-w-0">
                  <ItemTitle className="w-full">
                    <span className="truncate">{pipeline.project_name}</span>
                    <span className="truncate font-normal text-muted-foreground">
                      {pipeline.pipeline_name}
                    </span>
                  </ItemTitle>
                  <ItemDescription className="line-clamp-1">
                    {failingDescription(pipeline)}
                  </ItemDescription>
                </ItemContent>
                <ItemActions>
                  <Badge
                    variant={
                      pipeline.current_streak > 0 ? 'destructive' : 'outline'
                    }
                  >
                    #{pipeline.last_failed_build_number}
                  </Badge>
                </ItemActions>
              </Item>
            ))}
          </ItemGroup>
        )}
      </CardContent>
    </Card>
  )
}

/** Home's build health: headline metrics, the daily trend, and what keeps failing. */
export function DashboardBuildHealth({
  days,
  onDaysChange,
  onlineRunners,
  totalRunners,
}: {
  days: BuildInsightsWindow
  onDaysChange: (days: BuildInsightsWindow) => void
  onlineRunners?: number
  totalRunners?: number
}) {
  const insightsQuery = useBuildInsights({ days })
  const insights = insightsQuery.data

  return (
    <section className="flex flex-col gap-3" aria-labelledby="build-health">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h2
          id="build-health"
          className="text-sm font-medium text-muted-foreground"
        >
          Build health
        </h2>
        <ToggleGroup
          aria-label="Period"
          value={[String(days)]}
          variant="outline"
          size="sm"
          onValueChange={(value) => {
            const next = WINDOWS.find(
              (window) => String(window) === value.at(0),
            )
            if (next) onDaysChange(next)
          }}
        >
          {WINDOWS.map((window) => (
            <ToggleGroupItem
              key={window}
              value={String(window)}
              aria-label={`Last ${window} days`}
            >
              {window}d
            </ToggleGroupItem>
          ))}
        </ToggleGroup>
      </div>

      {insightsQuery.error && !insights ? (
        <Alert variant="destructive">
          <AlertDescription className="flex items-center justify-between gap-3">
            <span>Build health could not be loaded.</span>
            <Button
              variant="outline"
              size="sm"
              onClick={() => void insightsQuery.refetch()}
            >
              Retry
            </Button>
          </AlertDescription>
        </Alert>
      ) : !insights ? (
        <div
          className="flex flex-col gap-3"
          role="status"
          aria-label="Loading build health"
        >
          <div className="grid grid-cols-1 gap-3 @xl/main:grid-cols-2 @5xl/main:grid-cols-4">
            {Array.from({ length: 4 }, (_, index) => (
              <Skeleton key={index} className="h-28 w-full" />
            ))}
          </div>
          <Skeleton className="h-56 w-full" />
        </div>
      ) : (
        <div
          className={cn(
            'flex flex-col gap-3 transition-opacity',
            insightsQuery.isPlaceholderData && 'opacity-60',
          )}
        >
          <BuildMetrics
            insights={insights}
            onlineRunners={onlineRunners}
            totalRunners={totalRunners}
          />
          <div className="grid grid-cols-1 gap-3 @5xl/main:grid-cols-3">
            <div className="@5xl/main:col-span-2">
              <BuildTrend insights={insights} />
            </div>
            <FailingPipelines pipelines={insights.failing_pipelines} />
          </div>
        </div>
      )}
    </section>
  )
}
