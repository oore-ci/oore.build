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
import { DashboardSection } from '@/components/dashboard-section'
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
import { LIST_NAV_ITEM } from '@/lib/keyboard-shortcuts'
import { Stack } from '@/components/layout/stack'
import { focusRingVariants, stackVariants, textVariants } from '@/lib/recipes'

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
  days,
}: {
  current?: number | null
  previous?: number | null
  format: (difference: number) => string
  /** `null` when more or less is neither good nor bad, like build volume. */
  goodWhen: 'up' | 'down' | null
  days: number
}) {
  const prior = `vs prior ${days} days`
  if (current == null || previous == null) {
    return <Badge variant="outline">New</Badge>
  }
  const difference = current - previous
  const text = format(Math.abs(difference))
  if (text === format(0)) return <Badge variant="outline">No change</Badge>
  const up = difference > 0
  const variant =
    goodWhen == null
      ? 'outline'
      : up === (goodWhen === 'up')
        ? 'success'
        : 'destructive'
  return (
    <Badge
      variant={variant}
      aria-label={`${up ? 'Up' : 'Down'} ${text} ${prior}`}
    >
      <HugeiconsIcon
        icon={up ? ArrowUp01Icon : ArrowDown01Icon}
        data-icon="inline-start"
      />
      {up ? '+' : '−'}
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
  badge?: React.ReactNode
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
      className={cn(focusRingVariants(), 'rounded-xl')}
    >
      <Card size="sm" className="@container/card h-full">
        <CardHeader>
          <CardDescription>{title}</CardDescription>
          <CardTitle className="text-2xl font-semibold tabular-nums @[250px]/card:text-3xl">
            {value}
          </CardTitle>
          {badge ? <CardAction>{badge}</CardAction> : null}
        </CardHeader>
        <CardFooter className="justify-between gap-2 text-muted-foreground">
          <span className="truncate" title={footer}>
            {footer}
          </span>
          <HugeiconsIcon icon={ChevronRightIcon} className="shrink-0" />
        </CardFooter>
      </Card>
    </Link>
  )
}

function BuildMetrics({
  canApprove,
  insights,
  onlineRunners,
}: {
  canApprove: boolean
  insights: BuildInsightsResponse
  onlineRunners?: number
}) {
  const { totals, previous_totals: previous, queue } = insights
  const days = insights.window_days
  const failures = totals.failed + totals.timed_out
  const verdicts = totals.succeeded + failures
  const unsettled = totals.canceled + totals.expired
  const window = `${days} days`
  const waitText =
    queue.oldest_waiting_since != null
      ? `oldest ${relativeTime(queue.oldest_waiting_since)}`
      : insights.queue_wait.p50_seconds != null
        ? `median wait ${formatDuration(insights.queue_wait.p50_seconds)}`
        : null
  const queueFooter = [`${queue.running} running`, waitText]
    .filter(Boolean)
    .join(' · ')

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
            days={days}
          />
        }
        footer={
          verdicts === 0
            ? `No passed or failed builds in ${window}`
            : `${totals.succeeded} of ${verdicts} passed`
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
            goodWhen={null}
            days={days}
          />
        }
        footer={
          unsettled === 0
            ? `${failures} failed`
            : `${failures} failed · ${unsettled} canceled`
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
            days={days}
          />
        }
        footer={
          insights.duration.p90_seconds == null
            ? 'Passed builds'
            : `p90 ${formatDuration(insights.duration.p90_seconds)} · passed builds`
        }
        to="/builds"
        search={{ status: 'succeeded' }}
        label={`Median build time ${formatSeconds(insights.duration.p50_seconds)}. Open succeeded builds`}
      />
      <MetricCard
        title="Waiting now"
        value={String(queue.waiting)}
        badge={
          canApprove && queue.awaiting_approval > 0 ? (
            <Badge variant="outline">
              {queue.awaiting_approval} to approve
            </Badge>
          ) : undefined
        }
        footer={queueFooter}
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
        label={`${queue.waiting} builds waiting, ${queueFooter}`}
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
  { key: 'succeeded', label: 'Passed', className: 'bg-success/70' },
  { key: 'failed', label: 'Failed', className: 'bg-destructive/70' },
  { key: 'other', label: 'Canceled', className: 'bg-muted-foreground/30' },
] as const

/** A round axis top and step so ticks land on readable numbers. */
type AxisScale = { top: number; step: number }

function axisScale(max: number): AxisScale {
  const rough = Math.max(1, max) / 3
  const magnitude = 10 ** Math.floor(Math.log10(rough))
  const step =
    [1, 2, 5, 10].map((factor) => factor * magnitude).find((v) => v >= rough) ??
    10 * magnitude
  return { top: Math.ceil(Math.max(1, max) / step) * step, step }
}

/** Finished builds per day, stacked by outcome. */
function BuildTrend({ insights }: { insights: BuildInsightsResponse }) {
  const days = insights.daily
  const { top: max, step } = axisScale(
    Math.max(0, ...days.map((day) => day.succeeded + day.failed + day.other)),
  )
  const ticks = Array.from(
    { length: max / step + 1 },
    (_, index) => max - index * step,
  )
  const middle = days[Math.floor(days.length / 2)]

  return (
    <Card size="sm" className="h-full">
      <CardHeader>
        <CardTitle>
          <h3>Builds per day</h3>
        </CardTitle>
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
            className={cn(
              textVariants({ variant: 'caption' }),
              '-my-2 flex w-6 flex-col justify-between text-right tabular-nums',
            )}
            aria-hidden
          >
            {ticks.map((tick) => (
              <span key={tick} className="leading-4">
                {tick}
              </span>
            ))}
          </div>
          <div
            className="pointer-events-none absolute inset-y-0 right-0 left-8 flex flex-col justify-between"
            aria-hidden
          >
            {ticks.map((tick) => (
              <span key={tick} className="h-px w-full bg-border/70" />
            ))}
          </div>
          <div
            className={cn(
              'relative flex flex-1 items-end',
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
                        className={cn(
                          stackVariants(),
                          focusRingVariants(),
                          'group h-full min-w-0 flex-1 items-center justify-end rounded-sm',
                        )}
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
          className={cn(
            textVariants({ variant: 'caption' }),
            'flex justify-between pl-8',
          )}
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
  const now =
    pipeline.current_streak > 1
      ? `Failed ${pipeline.current_streak} in a row`
      : pipeline.current_streak === 1
        ? 'Last build failed'
        : `Last failed ${relativeTime(pipeline.last_failed_at)}`
  return `${now} · ${pipeline.failures} of ${pipeline.finished} failed`
}

/**
 * Pipelines with any failure in the period, failing ones first. Each says
 * whether it is failing now or has recovered, so a passing pipeline never
 * reads as broken.
 */
function FailingPipelines({
  pipelines,
}: {
  pipelines: Array<FailingPipelineInsights>
}) {
  return (
    <Card size="sm" className="h-full">
      <CardHeader>
        <CardTitle>
          <h3>Pipelines with failures</h3>
        </CardTitle>
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
                size="xs"
                className="-mx-2.5"
                render={
                  <Link
                    {...LIST_NAV_ITEM}
                    to="/builds/$buildId"
                    params={{ buildId: pipeline.last_failed_build_id }}
                  />
                }
              >
                <ItemContent className="min-w-0">
                  <ItemTitle className="w-full min-w-0">
                    <span className="truncate">
                      {pipeline.project_name}
                      <span className="font-normal text-muted-foreground">
                        {' '}
                        · {pipeline.pipeline_name}
                      </span>
                    </span>
                  </ItemTitle>
                  <ItemDescription className="line-clamp-1">
                    {failingDescription(pipeline)}
                  </ItemDescription>
                </ItemContent>
                <ItemActions>
                  {pipeline.current_streak > 0 ? (
                    <Badge variant="destructive">Failing</Badge>
                  ) : (
                    <Badge variant="outline">Recovered</Badge>
                  )}
                </ItemActions>
              </Item>
            ))}
          </ItemGroup>
        )}
      </CardContent>
    </Card>
  )
}

/** Home's build health over the chosen period: headline metrics, the daily trend, and what failed. */
export function DashboardBuildHealth({
  canApprove,
  days,
  onDaysChange,
  onlineRunners,
}: {
  canApprove: boolean
  days: BuildInsightsWindow
  onDaysChange: (days: BuildInsightsWindow) => void
  onlineRunners?: number
}) {
  const insightsQuery = useBuildInsights({ days })
  const insights = insightsQuery.data

  return (
    <DashboardSection
      title="Build health"
      scope={`Last ${days} days, changes vs the ${days} before`}
      action={
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
      }
    >
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
        <Stack gap={3} role="status" aria-label="Loading build health">
          <div className="grid grid-cols-1 gap-3 @xl/main:grid-cols-2 @5xl/main:grid-cols-4">
            {Array.from({ length: 4 }, (_, index) => (
              <Skeleton key={index} className="h-28 w-full" />
            ))}
          </div>
          <Skeleton className="h-56 w-full" />
        </Stack>
      ) : (
        <div
          className={cn(
            stackVariants({ gap: 3 }),
            'transition-opacity',
            insightsQuery.isPlaceholderData && 'opacity-60',
          )}
        >
          <BuildMetrics
            canApprove={canApprove}
            insights={insights}
            onlineRunners={onlineRunners}
          />
          <div className="grid grid-cols-1 gap-3 @5xl/main:grid-cols-3">
            <div className="@5xl/main:col-span-2">
              <BuildTrend insights={insights} />
            </div>
            <FailingPipelines pipelines={insights.failing_pipelines} />
          </div>
        </div>
      )}
    </DashboardSection>
  )
}
