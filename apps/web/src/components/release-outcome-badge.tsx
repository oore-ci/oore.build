import type { Build } from '@oore/client/models'
import { StatusBadge } from '@/components/status-badge'

/**
 * Where a build's store release stands, as a second chip beside its status.
 * A build succeeds once the store has the upload, so "Succeeded" alone says
 * nothing about review or release. Nothing shows for a build with no release.
 */
export function ReleaseOutcomeBadge({
  build,
  className,
}: {
  build: Pick<Build, 'release_outcome'>
  className?: string
}) {
  if (!build.release_outcome) return null
  return (
    <StatusBadge
      kind="release_outcome"
      value={build.release_outcome}
      className={className}
    />
  )
}
