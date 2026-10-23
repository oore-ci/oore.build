import type {
  StoreAppVitals,
  StoreErrorIssue,
  StoreVital,
  StoreVitalKind,
} from '@oore/client/models'
import { Link } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import { ArrowDown01Icon, Refresh01Icon } from '@hugeicons/core-free-icons'

import { isOoreApiError } from '@oore/client/client'
import type { ReleaseHealth } from '@oore/client/models'
import {
  DashboardList,
  DashboardSection,
  DashboardSectionError,
} from '@/components/dashboard-section'
import { Alert, AlertDescription } from '@/components/ui/alert'
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

function formatVital(value: number | null | undefined, unit: string): string {
  if (value == null) return '—'
  if (unit === 'ratio') return `${(value * 100).toFixed(2)}%`
  const rounded = value >= 100 ? Math.round(value) : value.toFixed(1)
  return unit ? `${rounded} ${unit}` : String(rounded)
}

type AppStatus = { label: string; variant: BadgeVariant }

/** The worst vital decides, in the same words the release board uses. */
function appStatus(app: StoreAppVitals): AppStatus {
  const levels = app.vitals.map((vital) =>
    healthFromThreshold(vital.value, vital.threshold),
  )
  const hasNumbers = app.vitals.some((vital) => vital.value != null)
  if (app.error && !hasNumbers)
    return { label: 'Not readable', variant: 'destructive' }
  const worst: ReleaseHealth = levels.includes('breached')
    ? 'breached'
    : levels.includes('warning')
      ? 'warning'
      : hasNumbers
        ? 'healthy'
        : 'unknown'
  return {
    label: RELEASE_HEALTH_LABEL[worst],
    variant: getReleaseHealthVariant(worst),
  }
}

/** A small line of the vital's history with the threshold drawn dashed. */
function Sparkline({ vital }: { vital: StoreVital }) {
  const values = vital.points.flatMap((point) =>
    point.value == null ? [] : [point.value],
  )
  if (values.length < 2) return null
  const width = 64
  const height = 20
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
      className="h-5 w-16 shrink-0 text-muted-foreground"
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

const LEVEL_DOT = {
  healthy: 'bg-success',
  warning: 'bg-warning',
  breached: 'bg-destructive',
  unknown: 'bg-muted-foreground/40',
} satisfies Record<ReleaseHealth, string>

function VitalStat({ vital }: { vital: StoreVital }) {
  const level = healthFromThreshold(vital.value, vital.threshold)
  const first = vital.points[0]?.label
  const last = vital.points[vital.points.length - 1]?.label
  const perVersion = vital.unit !== 'ratio'
  const context =
    vital.threshold == null
      ? perVersion && last
        ? `Version ${last}, typical user`
        : '28-day average'
      : `Threshold ${formatVital(vital.threshold, vital.unit)}`
  const history =
    first && last
      ? perVersion
        ? `By app version, ${first} to ${last}`
        : `Daily, ${first} to ${last}`
      : null

  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <Row
            tabIndex={0}
            gap={3}
            className={cn(focusRingVariants(), 'min-w-0 rounded-md')}
          />
        }
      >
        <div className="flex min-w-0 flex-col">
          <span
            className={cn(textVariants({ variant: 'caption' }), 'truncate')}
          >
            {VITAL_LABELS[vital.kind]}
          </span>
          <span className="flex items-center gap-1.5 text-sm font-medium tabular-nums">
            {vital.threshold == null ? null : (
              <span
                className={cn(
                  'size-1.5 shrink-0 rounded-full',
                  LEVEL_DOT[level],
                )}
                aria-hidden
              />
            )}
            {formatVital(vital.value, vital.unit)}
            {vital.threshold == null ? null : (
              <span className="font-normal text-muted-foreground">
                / {formatVital(vital.threshold, vital.unit)}
              </span>
            )}
          </span>
        </div>
        <Sparkline vital={vital} />
      </TooltipTrigger>
      <TooltipContent>
        <span className="flex flex-col gap-0.5">
          <span>
            {RELEASE_HEALTH_LABEL[level]} · {context}
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
        {issue.reports.toLocaleString()} reports ·{' '}
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
      className={cn(className, 'hover:bg-muted')}
    >
      {content}
    </a>
  ) : (
    <div className={className}>{content}</div>
  )
}

function AppRow({ app }: { app: StoreAppVitals }) {
  const status = appStatus(app)
  const issues = app.issues.slice(0, 3)
  const storeLabel = RELEASE_STORE_LABEL[app.store]
  return (
    <Stack
      id={storeHealthAnchor(app.project_id, app.store)}
      gap={3}
      className="scroll-mt-20 p-4"
    >
      <Stack gap={3} className="@3xl/main:flex-row @3xl/main:items-center">
        <div className="flex min-w-0 items-start justify-between gap-3 @3xl/main:w-64 @3xl/main:shrink-0">
          <div className="flex min-w-0 flex-col">
            <span className="truncate text-sm font-medium">
              {app.project_name} · {storeLabel}
            </span>
            <span
              className={cn(textVariants({ variant: 'caption' }), 'truncate')}
            >
              {app.app_identifier}
              {app.refreshed_at == null
                ? ''
                : ` · read ${relativeTime(app.refreshed_at)}`}
            </span>
          </div>
          <Badge variant={status.variant} className="@3xl/main:hidden">
            {status.label}
          </Badge>
        </div>
        {app.vitals.length > 0 ? (
          // oxlint-disable-next-line layout/max-class-utilities -- responsive vitals grid
          <div className="grid min-w-0 flex-1 grid-cols-1 gap-x-8 gap-y-3 sm:grid-cols-2 @5xl/main:flex @5xl/main:flex-wrap">
            {app.vitals.map((vital) => (
              <VitalStat key={vital.kind} vital={vital} />
            ))}
          </div>
        ) : !app.error ? (
          <Text variant="caption" className="flex-1">
            Oore reads this app's numbers within a few minutes of its first
            release.
          </Text>
        ) : (
          <div className="flex-1" />
        )}
        <Badge
          variant={status.variant}
          className="hidden shrink-0 @3xl/main:inline-flex"
        >
          {status.label}
        </Badge>
      </Stack>
      {app.error ? (
        <Alert variant="destructive">
          <AlertDescription className="flex flex-col gap-2 @3xl/main:flex-row @3xl/main:items-center @3xl/main:justify-between">
            <span>{app.error}</span>
            <span className="flex shrink-0 gap-2">
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
            </span>
          </AlertDescription>
        </Alert>
      ) : null}
      {issues.length > 0 ? (
        <Collapsible>
          <CollapsibleTrigger
            render={
              <Button
                variant="ghost"
                size="xs"
                className="-ms-2.5 text-muted-foreground"
              />
            }
          >
            Top issues, last 28 days ({issues.length})
            <HugeiconsIcon
              icon={ArrowDown01Icon}
              data-icon="inline-end"
              className="transition-transform in-data-open:rotate-180"
            />
          </CollapsibleTrigger>
          <CollapsibleContent>
            <div className="-mx-2 flex flex-col pt-1">
              {issues.map((issue) => (
                <IssueRow key={`${issue.kind}-${issue.cause}`} issue={issue} />
              ))}
            </div>
          </CollapsibleContent>
        </Collapsible>
      ) : null}
    </Stack>
  )
}

/** Crash, ANR, hang and launch numbers from the stores, one row per released app. */
export function DashboardStoreHealth() {
  const vitalsQuery = useStoreVitals()
  const refresh = useRefreshStoreVitals()
  const apps = vitalsQuery.data?.apps ?? []
  // Nothing released yet, or the daemon predates store vitals: stay out of
  // the way, but only once the daemon has answered.
  const unsupported =
    isOoreApiError(vitalsQuery.error) && vitalsQuery.error.status === 404
  if ((vitalsQuery.isSuccess && apps.length === 0) || unsupported) return null

  return (
    <DashboardSection
      title="Store health"
      scope="All versions"
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
        <DashboardList>
          {apps.map((app) => ({
            key: `${app.project_id}-${app.store}-${app.app_identifier}`,
            node: <AppRow app={app} />,
          }))}
        </DashboardList>
      )}
    </DashboardSection>
  )
}
