import { Badge } from '@/components/ui/badge'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { Skeleton } from '@/components/ui/skeleton'
import type { RetentionCleanupSummary } from '@/lib/types'

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
  return (
    <section className="border bg-card" aria-labelledby="last-cleanup-title">
      <div className="border-b px-4 py-3">
        <h2 id="last-cleanup-title" className="text-sm font-semibold">
          Last cleanup
        </h2>
      </div>
      <div className="p-4">
        {isLoading ? (
          <div className="space-y-2">
            <Skeleton className="h-4 w-64" />
            <Skeleton className="h-4 w-48" />
          </div>
        ) : error ? (
          <Alert variant="destructive">
            <AlertDescription className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
              <span>Failed to load the last cleanup: {error.message}</span>
              <Button
                type="button"
                variant="outline"
                size="sm"
                onClick={onRetry}
              >
                Retry
              </Button>
            </AlertDescription>
          </Alert>
        ) : lastCleanup ? (
          <div className="grid gap-4 sm:grid-cols-4">
            <div>
              <p className="text-sm text-muted-foreground">Builds cleaned</p>
              <p className="text-lg font-semibold">
                {lastCleanup.builds_expired}
              </p>
            </div>
            <div>
              <p className="text-sm text-muted-foreground">Artifacts deleted</p>
              <p className="text-lg font-semibold">
                {lastCleanup.artifacts_deleted}
              </p>
            </div>
            <div>
              <p className="text-sm text-muted-foreground">Space reclaimed</p>
              <p className="text-lg font-semibold">
                {formatBytes(lastCleanup.bytes_reclaimed)}
              </p>
            </div>
            <div>
              <p className="text-sm text-muted-foreground">Ran</p>
              <p className="text-lg font-semibold">
                {formatRelativeTime(lastCleanup.ran_at)}
                {lastCleanup.dry_run && (
                  <Badge variant="outline" className="ml-2">
                    Dry run
                  </Badge>
                )}
              </p>
            </div>
          </div>
        ) : (
          <p className="text-sm text-muted-foreground">
            No cleanup has run yet. Enable the retention policy and wait for the
            next scheduled run.
          </p>
        )}
      </div>
    </section>
  )
}
