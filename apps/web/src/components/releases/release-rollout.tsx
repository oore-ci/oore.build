import type { Release } from '@oore/client/models'
import { Badge } from '@/components/ui/badge'
import { Progress } from '@/components/ui/progress'
import {
  formatRolloutPercent,
  getReleaseStateVariant,
  RELEASE_STATE_LABEL,
} from '@/lib/releases'
import { cn } from '@/lib/utils'
import { Row } from '@/components/layout/row'
import { stackVariants } from '@/lib/recipes'

export function ReleaseRollout({
  className,
  release,
}: {
  className?: string
  release: Release
}) {
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
