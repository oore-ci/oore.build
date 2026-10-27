import { HugeiconsIcon } from '@hugeicons/react'
import { HourglassIcon } from '@hugeicons/core-free-icons'
import type { Release } from '@oore/client/models'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  getReleaseStateVariant,
  isWaitingOnStore,
  releaseStatusPhrase,
  releaseWaitSummary,
} from '@/lib/releases'

/**
 * Says what a succeeded build's release is still waiting on. A build
 * succeeds once the store has the upload, but the app reaches nobody until
 * the store finishes, like Codemagic's post-processing and EAS Submit.
 */
export function StoreWaitAlert({
  releases,
  onOpenRelease,
}: {
  releases: Array<Release>
  /** Absent when the Release tab is already open. */
  onOpenRelease?: () => void
}) {
  const waiting = releases.flatMap((release) => {
    const summary = isWaitingOnStore(release)
      ? releaseWaitSummary(release)
      : null
    return summary ? [{ release, summary }] : []
  })
  if (!waiting.length) return null

  return (
    <>
      {waiting.map(({ release, summary }) => (
        <Alert key={release.id} variant="default">
          <HugeiconsIcon icon={HourglassIcon} size={16} />
          <AlertTitle role="status">{summary.title}</AlertTitle>
          <AlertDescription className="flex min-w-0 flex-col items-start gap-2">
            <span>{summary.description}</span>
            {onOpenRelease ? (
              <Button variant="outline" size="xs" onClick={onOpenRelease}>
                View release
              </Button>
            ) : null}
          </AlertDescription>
        </Alert>
      ))}
    </>
  )
}

/** "App Store processing" beside the build's own status. */
export function StoreWaitBadges({
  releases,
  onOpenRelease,
}: {
  releases: Array<Release>
  onOpenRelease: () => void
}) {
  return releases.filter(isWaitingOnStore).map((release) => (
    <Badge
      key={release.id}
      variant={
        release.awaiting_approval
          ? 'warning'
          : getReleaseStateVariant(release.state)
      }
      className="h-6 px-2.5 text-[13px]"
      render={
        <button
          type="button"
          onClick={onOpenRelease}
          aria-label={`${releaseStatusPhrase(release)}. Open the Release tab`}
        />
      }
    >
      <PulseDot />
      {releaseStatusPhrase(release)}
    </Badge>
  ))
}

function PulseDot() {
  return (
    <span aria-hidden className="relative flex size-1.5">
      <span className="absolute inline-flex size-full rounded-full bg-current opacity-60 motion-safe:animate-ping" />
      <span className="relative inline-flex size-1.5 rounded-full bg-current" />
    </span>
  )
}
