import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Skeleton } from '@/components/ui/skeleton'
import type { RetentionCleanupSummary } from '@oore/client/models'

function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B'
  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  const i = Math.floor(Math.log(bytes) / Math.log(1024))
  return `${(bytes / Math.pow(1024, i)).toFixed(1)} ${units[i]}`
}

function formatRelativeTime(unixSecs: number): string {
  const now = Math.floor(Date.now() / 1000)
  const diff = now - unixSecs
  if (diff < 60) return 'just now'
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`
  return `${Math.floor(diff / 86400)}d ago`
}

export function RetentionSummaryCard({
  error,
  isLoading,
  lastCleanup,
  onRetry,
}: {
  error: Error | null
  isLoading: boolean
  lastCleanup: RetentionCleanupSummary | undefined
  onRetry: () => void
}) {
  if (isLoading) {
    return <Skeleton className="h-4 w-72" />
  }

  if (error) {
    return (
      <span
        role="alert"
        className="flex flex-wrap items-center gap-2 text-destructive"
      >
        <span>Last cleanup unavailable: {error.message}</span>
        <Button type="button" variant="ghost" size="xs" onClick={onRetry}>
          Retry
        </Button>
      </span>
    )
  }

  if (!lastCleanup) {
    return <span>No cleanup has run yet.</span>
  }

  return (
    <>
      <span>Last cleanup {formatRelativeTime(lastCleanup.ran_at)}</span>
      <span aria-hidden>·</span>
      <span>
        {lastCleanup.builds_expired} builds · {lastCleanup.artifacts_deleted}{' '}
        artifacts · {formatBytes(lastCleanup.bytes_reclaimed)} reclaimed
      </span>
      {lastCleanup.dry_run ? <Badge variant="outline">Dry run</Badge> : null}
    </>
  )
}
