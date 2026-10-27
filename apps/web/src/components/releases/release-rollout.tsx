import type { Release } from '@oore/client/models'
import { Badge } from '@/components/ui/badge'
import { Progress } from '@/components/ui/progress'
import {
  formatRolloutPercent,
  getReleaseStateVariant,
  isWaitingOnStore,
  RELEASE_STATE_LABEL,
  releaseStageIndex,
  releaseStages,
} from '@/lib/releases'
import { cn } from '@/lib/utils'
import { Row } from '@/components/layout/row'
import { stackVariants, textVariants } from '@/lib/recipes'

export function ReleaseRollout({
  className,
  release,
}: {
  className?: string
  release: Release
}) {
  // Before a rollout starts, 0% says nothing; show where the store is.
  if (isWaitingOnStore(release) && !release.awaiting_approval) {
    return <ReleaseStages release={release} className={className} />
  }

  const percent = formatRolloutPercent(release.rollout_fraction)

  return (
    <div className={cn(stackVariants({ gap: 1.5 }), 'min-w-36', className)}>
      <Row gap={2} className="justify-between">
        <ReleaseStateBadge release={release} />
        <span className="font-mono text-xs tabular-nums">{percent}</span>
      </Row>
      <Progress
        value={release.rollout_fraction * 100}
        aria-label={`Rollout ${percent} of users`}
        className={cn(
          release.state === 'halted' &&
            '[&_[data-slot=progress-indicator]]:bg-destructive',
          release.state === 'paused' &&
            '[&_[data-slot=progress-indicator]]:bg-warning',
        )}
      />
    </div>
  )
}

/** Segmented steps from upload to release, with the current one pulsing. */
function ReleaseStages({
  className,
  release,
}: {
  className?: string
  release: Release
}) {
  const stages = releaseStages(release)
  const current = releaseStageIndex(release)
  const next = stages[current + 1]

  return (
    <div className={cn(stackVariants({ gap: 1.5 }), 'min-w-36', className)}>
      <Row gap={2} className="justify-between">
        <ReleaseStateBadge release={release} />
        <span className={cn(textVariants({ variant: 'caption' }), 'text-xs')}>
          Step {current + 1} of {stages.length}
        </span>
      </Row>
      <ol
        className="flex gap-1"
        aria-label={`Step ${current + 1} of ${stages.length}: ${stages[current]}`}
      >
        {stages.map((stage, index) => (
          <li
            key={stage}
            aria-current={index === current ? 'step' : undefined}
            className={cn(
              'h-2 flex-1 rounded-2xl bg-muted',
              index < current && 'bg-primary',
              index === current && 'bg-info motion-safe:animate-pulse',
            )}
          >
            <span className="sr-only">
              {stage}
              {index < current ? ', done' : index === current ? ', now' : ''}
            </span>
          </li>
        ))}
      </ol>
      <span className={textVariants({ variant: 'caption' })}>
        {stages[current]}
        {next ? ` · then ${next}` : ''}
      </span>
    </div>
  )
}

export function ReleaseStateBadge({
  release,
}: {
  release: Pick<Release, 'awaiting_approval' | 'state'>
}) {
  return (
    <Badge variant={getReleaseStateVariant(release.state)}>
      {release.awaiting_approval
        ? 'Awaiting approval'
        : RELEASE_STATE_LABEL[release.state]}
    </Badge>
  )
}
