import type { BuildProgress } from '@/lib/build-identity'
import { remainingLabel } from '@/lib/build-identity'
import { Text } from '@/components/layout/text'

/**
 * A running build's place in a typical run: a bar for time spent against the
 * pipeline's median run, and "Step 3 of 6 · Building · about 4 min left".
 */
export function BuildProgressStrip({
  progress,
  stepLabel,
}: {
  progress: BuildProgress | null
  stepLabel: string | null
}) {
  if (!progress && !stepLabel) return null
  // Never full while it runs: a build past its typical time still is not done.
  const percent = progress
    ? Math.min(
        95,
        Math.max(
          2,
          ((progress.typicalSeconds - progress.remainingSeconds) /
            Math.max(1, progress.typicalSeconds)) *
            100,
        ),
      )
    : null
  const words = [
    progress?.step && progress.total
      ? `Step ${Math.min(progress.step, progress.total)} of ${progress.total}`
      : null,
    stepLabel,
    progress ? remainingLabel(progress.remainingSeconds) : null,
  ].filter(Boolean)

  return (
    <div className="space-y-1.5" role="status" aria-label="Build progress">
      {percent !== null ? (
        <div className="h-1 overflow-hidden rounded-full bg-muted" aria-hidden>
          <div
            className="h-full rounded-full bg-info/70 transition-[width] duration-1000"
            style={{ width: `${percent}%` }}
          />
        </div>
      ) : null}
      <Text variant="muted">{words.join(' · ')}</Text>
    </div>
  )
}
