import type { Release } from '@oore/client/models'
import { Badge } from '@/components/ui/badge'
import { Progress } from '@/components/ui/progress'
import {
  formatRolloutPercent,
  getReleaseStateVariant,
  RELEASE_STATE_LABEL,
} from '@/lib/releases'
import { cn } from '@/lib/utils'

export function ReleaseRollout({
  className,
  release,
}: {
  className?: string
  release: Release
}) {
  const percent = formatRolloutPercent(release.rollout_fraction)

  return (
    <div className={cn('flex min-w-36 flex-col gap-1.5', className)}>
      <div className="flex items-center justify-between gap-2">
        <ReleaseStateBadge release={release} />
        <span className="font-mono text-xs tabular-nums">{percent}</span>
      </div>
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
