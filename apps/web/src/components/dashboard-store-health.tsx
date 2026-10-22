import type {
  StoreAppVitals,
  StoreErrorIssue,
  StoreVital,
  StoreVitalKind,
} from '@oore/client/models'
import { Badge, type BadgeVariant } from '@/components/ui/badge'
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
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from '@/components/ui/tooltip'
import { useStoreVitals } from '@/hooks/use-releases'
import { relativeTime } from '@/lib/format-utils'
import { cn } from '@/lib/utils'

const VITAL_LABELS = {
  crash_rate: 'Crash rate',
  anr_rate: 'ANR rate',
  hang_rate: 'Hang rate',
  launch_time: 'Launch time',
} satisfies Record<StoreVitalKind, string>

const STORE_LABELS = {
  google_play: 'Google Play',
  app_store: 'App Store',
} as const

/** Above this share of the store's threshold a vital is worth watching. */
const NEAR_THRESHOLD = 0.75

type Level = 'good' | 'near' | 'over' | 'none'

function vitalLevel(vital: StoreVital): Level {
  if (vital.value == null) return 'none'
  if (vital.threshold == null || vital.threshold <= 0) return 'none'
  const share = vital.value / vital.threshold
  if (share >= 1) return 'over'
  return share >= NEAR_THRESHOLD ? 'near' : 'good'
}

function formatVital(value: number | null | undefined, unit: string): string {
  if (value == null) return '—'
  if (unit === 'ratio') return `${(value * 100).toFixed(2)}%`
  const rounded = value >= 100 ? Math.round(value) : value.toFixed(1)
  return unit ? `${rounded} ${unit}` : String(rounded)
}

type AppStatus = { label: string; variant: BadgeVariant }

function appStatus(app: StoreAppVitals): AppStatus {
  const levels = app.vitals.map(vitalLevel)
  const hasNumbers = app.vitals.some((vital) => vital.value != null)
  if (app.error && !hasNumbers)
    return { label: 'Not readable', variant: 'destructive' }
  if (levels.includes('over'))
    return { label: 'Over threshold', variant: 'destructive' }
  if (levels.includes('near'))
    return { label: 'Near threshold', variant: 'warning' }
  if (!hasNumbers) return { label: 'Waiting for data', variant: 'outline' }
  return { label: 'Healthy', variant: 'success' }
}

const LEVEL_TEXT = {
  good: 'text-success',
  near: 'text-warning',
  over: 'text-destructive',
  none: 'text-foreground',
} satisfies Record<Level, string>

/** A small line of the vital's history with the threshold drawn dashed. */
function Sparkline({ vital, level }: { vital: StoreVital; level: Level }) {
  const values = vital.points.flatMap((point) =>
    point.value == null ? [] : [point.value],
  )
  if (values.length < 2) return null
  const width = 120
  const height = 32
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
      className={cn('h-8 w-full', LEVEL_TEXT[level])}
      aria-hidden
    >
      {vital.threshold == null ? null : (
        <line
          x1={0}
          x2={width}
          y1={y(vital.threshold)}
          y2={y(vital.threshold)}
          className="stroke-muted-foreground/50"
          strokeDasharray="3 3"
          vectorEffect="non-scaling-stroke"
        />
      )}
      <polyline
        points={path}
        fill="none"
        stroke="currentColor"
        strokeWidth={1.5}
        strokeLinejoin="round"
        vectorEffect="non-scaling-stroke"
      />
    </svg>
  )
}

function VitalTile({ vital }: { vital: StoreVital }) {
  const level = vitalLevel(vital)
  const first = vital.points[0]?.label
  const last = vital.points[vital.points.length - 1]?.label
  const perVersion = vital.unit !== 'ratio'
  return (
    <div className="flex min-w-0 flex-col gap-1 rounded-lg border p-3">
      <span className="text-xs text-muted-foreground">
        {VITAL_LABELS[vital.kind]}
      </span>
      <span
        className={cn(
          'font-mono text-lg font-semibold tabular-nums',
          LEVEL_TEXT[level],
        )}
      >
        {formatVital(vital.value, vital.unit)}
      </span>
      <Tooltip>
        <TooltipTrigger render={<div className="w-full" />}>
          <Sparkline vital={vital} level={level} />
        </TooltipTrigger>
        <TooltipContent>
          {perVersion
            ? `By app version, ${first} to ${last}`
            : `Daily, ${first} to ${last}`}
        </TooltipContent>
      </Tooltip>
      <span className="text-xs text-muted-foreground">
        {vital.threshold == null
          ? perVersion && last
            ? `Version ${last}, typical user`
            : '28-day average'
          : `Threshold ${formatVital(vital.threshold, vital.unit)}`}
      </span>
    </div>
  )
}

function IssueRow({ issue }: { issue: StoreErrorIssue }) {
  const content = (
    <>
      <div className="flex min-w-0 flex-col">
        <span className="truncate font-mono text-xs">{issue.cause}</span>
        <span className="truncate text-xs text-muted-foreground">
          {issue.kind === 'anr' ? 'ANR' : 'Crash'}
          {issue.location ? ` · ${issue.location}` : ''}
        </span>
      </div>
      <span className="shrink-0 text-right font-mono text-xs text-muted-foreground tabular-nums">
        {issue.reports.toLocaleString()} reports
        <br />
        {issue.users.toLocaleString()} users
      </span>
    </>
  )
  const className =
    '-mx-2 flex items-center justify-between gap-3 rounded-md px-2 py-1.5'
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

function AppCard({ app }: { app: StoreAppVitals }) {
  const status = appStatus(app)
  const issues = app.issues.slice(0, 3)
  return (
    <Card size="sm" className="h-full">
      <CardHeader>
        <CardTitle>{app.project_name}</CardTitle>
        <CardDescription className="truncate">
          {STORE_LABELS[app.store]} · {app.app_identifier}
        </CardDescription>
        <CardAction>
          <Badge variant={status.variant}>{status.label}</Badge>
        </CardAction>
      </CardHeader>
      <CardContent className="flex flex-1 flex-col gap-3">
        {app.vitals.length > 0 ? (
          <div className="grid grid-cols-2 gap-2">
            {app.vitals.map((vital) => (
              <VitalTile key={vital.kind} vital={vital} />
            ))}
          </div>
        ) : null}
        {app.error ? (
          <p className="text-xs text-destructive">{app.error}</p>
        ) : null}
        {app.vitals.length === 0 && !app.error ? (
          <p className="text-xs text-muted-foreground">
            Oore reads this app's numbers within a few minutes of its first
            release.
          </p>
        ) : null}
        {issues.length > 0 ? (
          <div className="flex flex-col">
            <span className="text-xs font-medium text-muted-foreground">
              Top issues, last 28 days
            </span>
            {issues.map((issue) => (
              <IssueRow key={`${issue.kind}-${issue.cause}`} issue={issue} />
            ))}
          </div>
        ) : null}
      </CardContent>
      {app.refreshed_at == null ? null : (
        <CardFooter className="text-xs text-muted-foreground">
          Read from {STORE_LABELS[app.store]} {relativeTime(app.refreshed_at)}
        </CardFooter>
      )}
    </Card>
  )
}

/** Crash, ANR, hang and launch numbers from the stores, one card per released app. */
export function DashboardStoreHealth() {
  const vitalsQuery = useStoreVitals()
  const apps = vitalsQuery.data?.apps ?? []
  // Nothing released yet, or the daemon predates store vitals: stay out of the way.
  if (apps.length === 0) return null

  return (
    <section className="flex flex-col gap-2" aria-labelledby="store-health">
      <h2
        id="store-health"
        className="text-sm font-medium text-muted-foreground"
      >
        Store health
      </h2>
      <div className="grid grid-cols-1 gap-3 @3xl:grid-cols-2 @6xl:grid-cols-3">
        {apps.map((app) => (
          <AppCard
            key={`${app.project_id}-${app.store}-${app.app_identifier}`}
            app={app}
          />
        ))}
      </div>
    </section>
  )
}
