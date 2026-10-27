import { HugeiconsIcon } from '@hugeicons/react'
import { Alert02Icon, HourglassIcon } from '@hugeicons/core-free-icons'
import type { Release } from '@oore/client/models'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  isWaitingOnStore,
  releaseStateBadgeVariant,
  releaseStatusPhrase,
  releaseStoreError,
  releaseWaitSummary,
} from '@/lib/releases'
import { monoVariants } from '@/lib/recipes'
import { cn } from '@/lib/utils'

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
      {waiting.map(({ release, summary }) => {
        const storeError = releaseStoreError(release)
        return (
          <Alert
            key={release.id}
            variant="default"
            className={storeError ? 'border-warning/40' : undefined}
          >
            <HugeiconsIcon
              icon={storeError ? Alert02Icon : HourglassIcon}
              size={16}
              className={storeError ? 'text-warning' : undefined}
            />
            <AlertTitle role={storeError ? 'alert' : 'status'}>
              {summary.title}
            </AlertTitle>
            <AlertDescription className="flex min-w-0 flex-col items-start gap-2">
              {storeError ? (
                <StoreErrorMessage message={storeError.message} />
              ) : null}
              <span>{summary.description}</span>
              {onOpenRelease ? (
                <Button variant="outline" size="xs" onClick={onOpenRelease}>
                  View release
                </Button>
              ) : null}
            </AlertDescription>
          </Alert>
        )
      })}
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
      variant={releaseStateBadgeVariant(release)}
      className="h-6 px-2.5 text-[13px]"
      render={
        <button
          type="button"
          onClick={onOpenRelease}
          aria-label={`${releaseStatusPhrase(release)}. Open the Release tab`}
        />
      }
    >
      {releaseStoreError(release) ? null : <PulseDot />}
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

/** The store's own words, which say what to fix. */
export function StoreErrorMessage({ message }: { message: string }) {
  return (
    <pre
      className={cn(
        monoVariants({ size: 'xs' }),
        'max-h-48 w-full overflow-auto rounded-lg bg-muted/60 p-3 whitespace-pre-wrap text-foreground',
      )}
    >
      {message}
    </pre>
  )
}
