import type {
  StoreAppVitals,
  StoreErrorIssue,
  StoreVital,
  StoreVitalKind,
} from '@oore/client/models'
import { useMemo } from 'react'
import { Link } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import { ArrowDown01Icon, Refresh01Icon } from '@hugeicons/core-free-icons'

import { isOoreApiError } from '@oore/client/client'
import type { ReleaseHealth } from '@oore/client/models'
import {
  DashboardSection,
  DashboardSectionError,
} from '@/components/dashboard-section'
import { Badge, type BadgeVariant } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible'
import { Skeleton } from '@/components/ui/skeleton'
import { Spinner } from '@/components/ui/spinner'
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from '@/components/ui/tooltip'
import { useRefreshStoreVitals, useStoreVitals } from '@/hooks/use-releases'
import { relativeTime } from '@/lib/format-utils'
import {
  getReleaseHealthVariant,
  healthFromThreshold,
  RELEASE_HEALTH_LABEL,
  RELEASE_STORE_LABEL,
} from '@/lib/releases'
import { toast } from '@/lib/toast'
import { cn } from '@/lib/utils'
import { Row } from '@/components/layout/row'
import { Stack } from '@/components/layout/stack'
import { Text } from '@/components/layout/text'
import { focusRingVariants, textVariants } from '@/lib/recipes'

const VITAL_LABELS = {
  crash_rate: 'Crash rate',
  anr_rate: 'ANR rate',
  hang_rate: 'Hang rate',
  launch_time: 'Launch time',
} satisfies Record<StoreVitalKind, string>

const DOCS_PATH = {
  google_play: 'build/distribute/google-play#store-health-on-home',
  app_store: 'build/distribute/app-store#store-health-on-home',
} satisfies Record<StoreAppVitals['store'], string>

/** The anchor a release row links to for the same app and store. */
export function storeHealthAnchor(projectId: string, store: string): string {
  return `store-health-${projectId}-${store}`
}

/** A vital as its number and unit, so the number can carry the weight. */
type VitalParts = { number: string; unit: string }

function vitalParts(
  value: number | null | undefined,
  unit: string,
): VitalParts {
  if (value == null) return { number: '—', unit: '' }
  if (unit === 'ratio') return { number: (value * 100).toFixed(2), unit: '%' }
  const rounded = value >= 100 ? String(Math.round(value)) : value.toFixed(1)
  return { number: rounded, unit }
}

function formatVital(value: number | null | undefined, unit: string): string {
  const parts = vitalParts(value, unit)
  if (!parts.unit) return parts.number
  return parts.unit === '%'
    ? `${parts.number}%`
    : `${parts.number} ${parts.unit}`
}

type AppLevel = ReleaseHealth | 'unreadable'

/** The worst vital decides; an app Oore can't read counts as a problem. */
function appLevel(app: StoreAppVitals): AppLevel {
  const levels = app.vitals.map((vital) =>
    healthFromThreshold(vital.value, vital.threshold),
  )
  const hasNumbers = app.vitals.some((vital) => vital.value != null)
  if (app.error && !hasNumbers) return 'unreadable'
  if (levels.includes('breached')) return 'breached'
  if (levels.includes('warning')) return 'warning'
  return hasNumbers ? 'healthy' : 'unknown'
}

const LEVEL_ORDER = {
  breached: 0,
  unreadable: 1,
  warning: 2,
  unknown: 3,
  healthy: 4,
} satisfies Record<AppLevel, number>

const LEVEL_STATUS = {
  breached: { label: RELEASE_HEALTH_LABEL.breached, variant: 'destructive' },
  unreadable: { label: 'Not readable', variant: 'destructive' },
  warning: {
    label: RELEASE_HEALTH_LABEL.warning,
    variant: getReleaseHealthVariant('warning'),
  },
  unknown: {
    label: RELEASE_HEALTH_LABEL.unknown,
    variant: getReleaseHealthVariant('unknown'),
  },
  healthy: {
    label: RELEASE_HEALTH_LABEL.healthy,
    variant: getReleaseHealthVariant('healthy'),
  },
} satisfies Record<AppLevel, { label: string; variant: BadgeVariant }>

const LEVEL_CARD = {
  breached: 'border-destructive/40',
  unreadable: 'border-destructive/40',
  warning: 'border-warning/40',
  unknown: '',
  healthy: '',
} satisfies Record<AppLevel, string>

/** Only numbers that need someone get colour; healthy ones stay quiet. */
const LEVEL_TEXT = {
  healthy: 'text-foreground',
  warning: 'text-warning',
  breached: 'text-destructive',
  unknown: 'text-foreground',
} satisfies Record<ReleaseHealth, string>

const LEVEL_FILL = {
  healthy: 'bg-foreground/35',
  warning: 'bg-warning',
  breached: 'bg-destructive',
  unknown: 'bg-muted-foreground/40',
} satisfies Record<ReleaseHealth, string>

/** A small line of the vital's history with the threshold drawn dashed. */
function Sparkline({ vital }: { vital: StoreVital }) {
  const values = vital.points.flatMap((point) =>
    point.value == null ? [] : [point.value],
  )
  if (values.length < 2) return null
  const width = 64
  const height = 24
  const top = Math.max(...values, vital.threshold ?? 0) * 1.1
  // Rates sit against their threshold from zero; per-version numbers show the change.
  const bottom = vital.threshold == null ? Math.min(...values) * 0.8 : 0
  const y = (value: number) =>
    height - ((value - bottom) / (top - bottom || 1)) * height
  const path = vital.points
    .flatMap((point, index) => {
      if (point.value == null) return []
      const x = (index / (vital.points.length - 1)) * width
      return [`${x.toFixed(1)},${y(point.value).toFixed(1)}`]
    })
    .join(' ')
  return (
    <svg
      viewBox={`0 0 ${width} ${height}`}
      preserveAspectRatio="none"
      className="h-6 w-16 shrink-0 text-muted-foreground"
      aria-hidden
    >
      {vital.threshold == null ? null : (
        <line
          x1={0}
          x2={width}
          y1={y(vital.threshold)}
          y2={y(vital.threshold)}
          className="stroke-muted-foreground/40"
          strokeDasharray="3 3"
          vectorEffect="non-scaling-stroke"
        />
      )}
      <polyline
        points={path}
        fill="none"
        stroke="currentColor"
        strokeWidth={1.25}
        strokeLinejoin="round"
        vectorEffect="non-scaling-stroke"
      />
    </svg>
  )
}

/** How far the number is toward the store's threshold, which sits at the tick. */
function ThresholdMeter({
  level,
  threshold,
  value,
}: {
  level: ReleaseHealth
  threshold: number
  value: number
}) {
  const scale = Math.max(threshold * 1.4, value)
  const percent = (part: number) => `${Math.min(100, (part / scale) * 100)}%`
  return (
    <div className="relative h-1.5 rounded-full bg-muted" aria-hidden>
      <div
        className={cn('h-full rounded-full', LEVEL_FILL[level])}
        style={{ width: percent(value) }}
      />
      <div
        className="absolute -inset-y-1 w-0.5 rounded-full bg-foreground/60"
        style={{ left: percent(threshold) }}
      />
    </div>
  )
}

/** Change across the versions shown; lower is better for every store vital. */
function versionChange(vital: StoreVital): string | null {
  const points = vital.points.filter((point) => point.value != null)
  const first = points[0]
  const last = points[points.length - 1]
  if (!first?.value || !last?.value || points.length < 2) return null
  const change = (last.value - first.value) / first.value
  if (Math.abs(change) < 0.01) return `Steady since ${first.label}`
  const amount = `${Math.round(Math.abs(change) * 100)}%`
  return change < 0
    ? `Down ${amount} since ${first.label}`
    : `Up ${amount} since ${first.label}`
}

function VitalTile({
  storeLabel,
  vital,
}: {
  storeLabel: string
  vital: StoreVital
}) {
  const level = healthFromThreshold(vital.value, vital.threshold)
  const parts = vitalParts(vital.value, vital.unit)
  const first = vital.points[0]?.label
  const last = vital.points[vital.points.length - 1]?.label
  const perVersion = vital.unit !== 'ratio'
  const threshold =
    vital.threshold == null ? null : formatVital(vital.threshold, vital.unit)
  const context =
    threshold == null
      ? ((perVersion ? versionChange(vital) : null) ??
        (perVersion && last ? `Version ${last}` : '28-day average'))
      : level === 'breached'
        ? `Over the ${threshold} threshold`
        : level === 'warning'
          ? `Near the ${threshold} threshold`
          : `Threshold ${threshold}`
  const history =
    first && last
      ? perVersion
        ? `By app version, ${first} to ${last}, typical user`
        : `Daily, ${first} to ${last}`
      : null

  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <Stack
            tabIndex={0}
            gap={2}
            className={cn(focusRingVariants(), 'min-w-0 rounded-md')}
          />
        }
      >
        <span className={cn(textVariants({ variant: 'caption' }), 'truncate')}>
          {VITAL_LABELS[vital.kind]}
        </span>
        <div className="flex items-end justify-between gap-2">
          <span
            className={cn(
              'text-2xl leading-none font-semibold tracking-tight tabular-nums',
              LEVEL_TEXT[level],
            )}
          >
            {parts.number}
            {parts.unit ? (
              <span
                className={cn(
                  'text-sm font-medium',
                  parts.unit === '%' ? 'ms-0.5' : 'ms-1',
                  level === 'healthy' || level === 'unknown'
                    ? 'text-muted-foreground'
                    : undefined,
                )}
              >
                {parts.unit}
              </span>
            ) : null}
          </span>
          <Sparkline vital={vital} />
        </div>
        {vital.threshold == null || vital.value == null ? null : (
          <ThresholdMeter
            level={level}
            threshold={vital.threshold}
            value={vital.value}
          />
        )}
        <span className={cn(textVariants({ variant: 'caption' }), 'truncate')}>
          {context}
        </span>
      </TooltipTrigger>
      <TooltipContent>
        <span className="flex flex-col gap-0.5">
          <span>
            {RELEASE_HEALTH_LABEL[level]}
            {threshold == null ? '' : ` · ${storeLabel} threshold ${threshold}`}
          </span>
          {history ? <span>{history}</span> : null}
        </span>
      </TooltipContent>
    </Tooltip>
  )
}

function IssueRow({ issue }: { issue: StoreErrorIssue }) {
  const kind = issue.kind === 'anr' ? 'ANR' : 'Crash'
  const content = (
    <>
      <div className="flex min-w-0 flex-col">
        <span className="truncate font-mono text-xs" title={issue.cause}>
          {issue.cause}
        </span>
        <span
          className={cn(textVariants({ variant: 'caption' }), 'truncate')}
          title={issue.location ?? undefined}
        >
          {kind}
          {issue.location ? ` · ${issue.location}` : ''}
        </span>
      </div>
      <span
        className={cn(
          textVariants({ variant: 'caption' }),
          'shrink-0 text-right tabular-nums',
        )}
      >
        {issue.users.toLocaleString()} users
      </span>
    </>
  )
  const className =
    'flex items-center justify-between gap-3 rounded-md px-2 py-1.5'
  return issue.url ? (
    <a
      href={issue.url}
      target="_blank"
      rel="noreferrer"
      aria-label={`Open ${issue.cause} in Play Console`}
      title={`${issue.reports.toLocaleString()} reports`}
      className={cn(className, 'hover:bg-muted')}
    >
      {content}
    </a>
  ) : (
    <div className={className}>{content}</div>
  )
}

function AppCard({ app, anchor }: { app: StoreAppVitals; anchor?: string }) {
  const level = appLevel(app)
  const status = LEVEL_STATUS[level]
  const issues = app.issues.slice(0, 3)
  const storeLabel = RELEASE_STORE_LABEL[app.store]
  const readAt =
    app.refreshed_at == null ? null : `Read ${relativeTime(app.refreshed_at)}`

  return (
    <Collapsible
      render={
        <Stack
          id={anchor}
          gap={4}
          className={cn(
            'min-w-0 scroll-mt-20 rounded-xl border bg-card p-4',
            LEVEL_CARD[level],
          )}
        />
      }
    >
      <div className="flex min-w-0 items-start justify-between gap-3">
        <div className="flex min-w-0 flex-col">
          <span className="truncate text-sm font-semibold">
            {app.project_name}
          </span>
          <span
            className={cn(textVariants({ variant: 'caption' }), 'truncate')}
            title={app.app_identifier}
          >
            {storeLabel} · {app.app_identifier}
          </span>
        </div>
        <Badge variant={status.variant} className="shrink-0">
          {status.label}
        </Badge>
      </div>

      {app.vitals.length > 0 ? (
        <div className="grid grid-cols-2 gap-x-6 gap-y-4">
          {app.vitals.map((vital) => (
            <VitalTile key={vital.kind} storeLabel={storeLabel} vital={vital} />
          ))}
        </div>
      ) : app.error ? null : (
        <Text variant="caption">
          Oore reads this app's numbers within a few minutes.
        </Text>
      )}

      {app.error ? (
        <Stack gap={3}>
          <Text variant="muted" className="text-destructive">
            {app.error}
          </Text>
          <Row gap={2}>
            <Button
              variant="outline"
              size="sm"
              nativeButton={false}
              render={<Link to="/settings/store-credentials" />}
            >
              Fix in Store credentials
            </Button>
            <Button
              variant="ghost"
              size="sm"
              nativeButton={false}
              render={
                <a
                  href={`https://docs.oore.build/${DOCS_PATH[app.store]}`}
                  target="_blank"
                  rel="noopener noreferrer"
                />
              }
            >
              Docs
            </Button>
          </Row>
        </Stack>
      ) : null}

      {issues.length > 0 || readAt ? (
        <div className="mt-auto flex min-h-6 items-center justify-between gap-3">
          {issues.length > 0 ? (
            <CollapsibleTrigger
              render={
                <Button
                  variant="ghost"
                  size="xs"
                  className="-ms-2 text-muted-foreground"
                />
              }
            >
              Top issues ({issues.length})
              <HugeiconsIcon
                icon={ArrowDown01Icon}
                data-icon="inline-end"
                className="transition-transform in-data-open:rotate-180"
              />
            </CollapsibleTrigger>
          ) : (
            <span />
          )}
          {readAt ? <Text variant="caption">{readAt}</Text> : null}
        </div>
      ) : null}
      {issues.length > 0 ? (
        <CollapsibleContent>
          <div className="-mx-2 -mt-3 flex flex-col">
            {issues.map((issue) => (
              <IssueRow key={`${issue.kind}-${issue.cause}`} issue={issue} />
            ))}
          </div>
        </CollapsibleContent>
      ) : null}
    </Collapsible>
  )
}

/** The section's one-line answer: how many apps need a look. */
function summarize(levels: Array<AppLevel>): string {
  const problems = levels.filter(
    (level) =>
      level === 'breached' || level === 'unreadable' || level === 'warning',
  ).length
  if (problems > 0) return `${problems} of ${levels.length} need attention`
  return levels.includes('healthy') ? 'All healthy' : 'All versions'
}

/** Crash, ANR, hang and launch numbers from the stores, one row per released or configured app. */
export function DashboardStoreHealth() {
  const vitalsQuery = useStoreVitals()
  const refresh = useRefreshStoreVitals()
  const apps = useMemo(
    () =>
      (vitalsQuery.data?.apps ?? [])
        .map((app) => ({ app, level: appLevel(app) }))
        // Stable, so apps keep the daemon's order within a level.
        .sort((a, b) => LEVEL_ORDER[a.level] - LEVEL_ORDER[b.level]),
    [vitalsQuery.data?.apps],
  )
  // Nothing released yet, or the daemon predates store vitals: stay out of
  // the way, but only once the daemon has answered.
  const unsupported =
    isOoreApiError(vitalsQuery.error) && vitalsQuery.error.status === 404
  if ((vitalsQuery.isSuccess && apps.length === 0) || unsupported) return null

  return (
    <DashboardSection
      title="Store health"
      scope={summarize(apps.map(({ level }) => level))}
      collapsedOnMobile
      action={
        <Button
          variant="ghost"
          size="sm"
          className="-me-2"
          disabled={refresh.isPending}
          onClick={() =>
            refresh.mutate(undefined, {
              onError: (error) =>
                toast.error(`Couldn't refresh store health: ${error.message}`),
            })
          }
        >
          {refresh.isPending ? (
            <Spinner data-icon="inline-start" />
          ) : (
            <HugeiconsIcon icon={Refresh01Icon} data-icon="inline-start" />
          )}
          {refresh.isPending ? 'Reading stores…' : 'Refresh'}
        </Button>
      }
    >
      {vitalsQuery.error ? (
        <DashboardSectionError
          label="Store health"
          onRetry={() => void vitalsQuery.refetch()}
        />
      ) : !vitalsQuery.data ? (
        <Skeleton className="h-36 w-full" />
      ) : (
        <div className="grid grid-cols-1 gap-3 @3xl/main:grid-cols-2 @7xl/main:grid-cols-3">
          {apps.map(({ app }, index) => {
            const anchor = storeHealthAnchor(app.project_id, app.store)
            // A project can have several apps in one store; links land on its first.
            const first =
              apps.findIndex(
                ({ app: other }) =>
                  storeHealthAnchor(other.project_id, other.store) === anchor,
              ) === index
            return (
              <AppCard
                key={`${app.project_id}-${app.store}-${app.app_identifier}`}
                app={app}
                anchor={first ? anchor : undefined}
              />
            )
          })}
        </div>
      )}
    </DashboardSection>
  )
}
