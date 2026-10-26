import { formatDuration } from '@/lib/format-utils'
import { formatStepDelta, stepTimingTrend } from '@/lib/step-timings'
import { cn } from '@/lib/utils'

const TREND_FILL = {
  slower: 'bg-warning',
  faster: 'bg-success',
  typical: 'bg-muted-foreground/40',
} as const

/** The step's time on a shared scale, with a tick at the pipeline median. */
export function StepDurationBar({
  durationMs,
  longestMs,
  medianMs,
}: {
  durationMs: number
  longestMs: number
  medianMs?: number
}) {
  const trend =
    medianMs != null ? stepTimingTrend(durationMs, medianMs) : 'typical'
  return (
    <div aria-hidden className="relative mt-1 h-1 w-full rounded-full bg-muted">
      <div
        className={cn('h-full rounded-full', TREND_FILL[trend])}
        style={{ width: `${Math.max(2, (durationMs / longestMs) * 100)}%` }}
      />
      {medianMs != null ? (
        <div
          className="absolute -top-0.5 h-2 w-px bg-foreground/60"
          style={{ left: `${Math.min(100, (medianMs / longestMs) * 100)}%` }}
        />
      ) : null}
    </div>
  )
}

/** The difference from the median, coloured when the step is off trend. */
export function StepMedianDelta({
  durationMs,
  medianMs,
}: {
  durationMs: number
  medianMs: number
}) {
  const trend = stepTimingTrend(durationMs, medianMs)
  const median = formatDuration(medianMs / 1000)
  return (
    <span
      className={cn(
        trend === 'slower' && 'text-warning',
        trend === 'faster' && 'text-success',
      )}
      title={`Median ${median} over recent successful builds of this pipeline`}
    >
      {formatStepDelta(durationMs, medianMs)}
      <span className="sr-only">
        {trend === 'typical'
          ? `, about the median of ${median}`
          : `, ${trend} than the median of ${median}`}
      </span>
    </span>
  )
}
