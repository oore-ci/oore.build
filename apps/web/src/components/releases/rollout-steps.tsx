import type { Release } from '@oore/client/models'
import { formatRolloutPercent } from '@/lib/releases'
import { cn } from '@/lib/utils'
import { Text } from '@/components/layout/text'

const HEIGHT = 64
const PAD = 6

/** "Tue 14:00" for a step a few days away. */
function stepTime(at: number) {
  return new Date(at * 1000).toLocaleString(undefined, {
    weekday: 'short',
    hour: 'numeric',
    minute: '2-digit',
  })
}

/**
 * A Google Play rollout schedule drawn as the stepped line it is: each step
 * of the policy, where the rollout is now, when the next raise happens, and
 * the crash rate that stops it.
 */
export function RolloutSteps({
  release,
}: {
  release: Pick<
    Release,
    | 'policy'
    | 'rollout_fraction'
    | 'next_fraction'
    | 'next_step_at'
    | 'state'
    | 'crash_rate_threshold'
  >
}) {
  const steps = release.policy.steps ?? []
  if (steps.length < 2) return null
  // Square root keeps 1% and 5% apart while 100% stays on top.
  const y = (fraction: number) =>
    PAD + (HEIGHT - 2 * PAD) * (1 - Math.sqrt(Math.min(1, fraction)))
  const reached = steps.filter(
    (step) => step <= release.rollout_fraction + 1e-9,
  ).length
  const current = Math.max(0, reached - 1)
  const halted = release.state === 'halted' || release.state === 'paused'
  const width = 100 / steps.length
  const points = steps.flatMap((step, index) => [
    `${index * width},${y(step)}`,
    `${(index + 1) * width},${y(step)}`,
  ])
  const done = points.slice(0, reached * 2)

  return (
    <figure
      className="flex flex-col gap-1.5"
      aria-label={`Rollout schedule: ${steps.map(formatRolloutPercent).join(', ')}, every ${release.policy.interval_hours}h. Now at ${formatRolloutPercent(release.rollout_fraction)}.`}
    >
      <svg
        viewBox={`0 0 100 ${HEIGHT}`}
        preserveAspectRatio="none"
        className="h-16 w-full overflow-visible"
        aria-hidden
      >
        <polyline
          points={points.join(' ')}
          fill="none"
          className="stroke-muted-foreground/30"
          strokeWidth={1.5}
          strokeDasharray="3 2"
          vectorEffect="non-scaling-stroke"
        />
        {done.length ? (
          <polyline
            points={done.join(' ')}
            fill="none"
            className={halted ? 'stroke-warning' : 'stroke-primary'}
            strokeWidth={2}
            vectorEffect="non-scaling-stroke"
          />
        ) : null}
      </svg>
      <ol
        className="grid text-[11px] leading-tight"
        style={{ gridTemplateColumns: `repeat(${steps.length}, 1fr)` }}
      >
        {steps.map((step, index) => {
          const isCurrent = index === current && reached > 0
          const isNext =
            index === reached && release.next_fraction != null && !halted
          return (
            <li
              key={step}
              aria-current={isCurrent ? 'step' : undefined}
              className={cn(
                'flex flex-col gap-0.5 tabular-nums',
                index < reached ? 'text-foreground' : 'text-muted-foreground',
              )}
            >
              <span className={cn(isCurrent && 'font-semibold')}>
                {formatRolloutPercent(step)}
              </span>
              {isCurrent ? (
                <span className="font-medium text-primary">
                  {halted ? 'stopped here' : 'you are here'}
                </span>
              ) : isNext && release.next_step_at ? (
                <span>{stepTime(release.next_step_at)}</span>
              ) : null}
            </li>
          )
        })}
      </ol>
      <Text variant="caption">
        Every {release.policy.interval_hours}h
        {release.crash_rate_threshold != null
          ? `, halts if crashes reach ${formatRolloutPercent(release.crash_rate_threshold)}`
          : ''}
      </Text>
    </figure>
  )
}
