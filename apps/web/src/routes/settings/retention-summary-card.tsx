import type { RetentionCleanupSummary } from '@/lib/types'
import { Badge } from '@/components/ui/badge'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Skeleton } from '@/components/ui/skeleton'

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
  isLoading,
  lastCleanup,
}: {
  isLoading: boolean
  lastCleanup: RetentionCleanupSummary | undefined
}) {
  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-sm font-medium uppercase tracking-wider text-muted-foreground">
          Last Cleanup
        </CardTitle>
      </CardHeader>
      <CardContent>
        {isLoading ? (
          <div className="space-y-2">
            <Skeleton className="h-4 w-64" />
            <Skeleton className="h-4 w-48" />
          </div>
        ) : lastCleanup ? (
          <div className="grid gap-4 sm:grid-cols-4">
            <div>
              <p className="text-muted-foreground text-sm">Builds cleaned</p>
              <p className="text-lg font-semibold">
                {lastCleanup.builds_expired}
              </p>
            </div>
            <div>
              <p className="text-muted-foreground text-sm">
                Artifacts deleted
              </p>
              <p className="text-lg font-semibold">
                {lastCleanup.artifacts_deleted}
              </p>
            </div>
            <div>
              <p className="text-muted-foreground text-sm">Space reclaimed</p>
              <p className="text-lg font-semibold">
                {formatBytes(lastCleanup.bytes_reclaimed)}
              </p>
            </div>
            <div>
              <p className="text-muted-foreground text-sm">Ran</p>
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
          <p className="text-muted-foreground text-sm">
            No cleanup has run yet. Enable the retention policy and wait for the
            next scheduled run.
          </p>
        )}
      </CardContent>
    </Card>
  )
}
