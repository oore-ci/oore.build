import { Suspense, lazy, useState } from 'react'
import { Link } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  Download04Icon,
  File01Icon,
  Share08Icon,
} from '@hugeicons/core-free-icons'
import { toast } from 'sonner'

import type { Artifact, BuildStatus } from '@/lib/types'
import { useArtifactDownloadLink } from '@/hooks/use-builds'
import { formatFileSize, relativeTime } from '@/lib/format-utils'
import { artifactInstallReadiness } from '@/lib/artifact-install'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Skeleton } from '@/components/ui/skeleton'
import { Spinner } from '@/components/ui/spinner'

const loadArtifactShareMenu = () => import('./artifact-share-menu')
const ArtifactShareMenu = lazy(loadArtifactShareMenu)

function artifactTypeBadgeVariant(type: Artifact['artifact_type']) {
  switch (type) {
    case 'apk':
      return 'info' as const
    case 'ipa':
      return 'success' as const
    case 'app':
      return 'warning' as const
    default:
      return 'secondary' as const
  }
}

function isArtifactExpired(artifact: Artifact): boolean {
  if (artifact.expires_at == null) return false
  return artifact.expires_at <= Math.floor(Date.now() / 1000)
}

function artifactExpiryLabel(artifact: Artifact): string | null {
  if (artifact.expires_at == null) return null
  const now = Math.floor(Date.now() / 1000)
  if (artifact.expires_at <= now) return 'Expired'
  return `Expires ${relativeTime(artifact.expires_at)}`
}

function artifactEmptyMessage(buildStatus: BuildStatus): string {
  switch (buildStatus) {
    case 'succeeded':
      return 'Build succeeded, but no files matched the pipeline artifact patterns.'
    case 'failed':
    case 'timed_out':
      return 'This build ended before it could publish artifacts.'
    case 'canceled':
      return 'This build was canceled before artifacts were published.'
    case 'expired':
      return 'Artifacts are no longer available for this expired build.'
    default:
      return 'Artifacts will appear here once the build produces them.'
  }
}

function ArtifactShareControl({ artifact }: { artifact: Artifact }) {
  const [requested, setRequested] = useState(false)
  const [open, setOpen] = useState(false)
  const expired = isArtifactExpired(artifact)

  const trigger = (
    <Button
      variant="outline"
      size="icon-xs"
      aria-label={`Share options for ${artifact.name}`}
      title="Share options"
      disabled={expired}
      onMouseEnter={() => void loadArtifactShareMenu()}
      onFocus={() => void loadArtifactShareMenu()}
      onClick={() => {
        setRequested(true)
        setOpen(true)
      }}
    >
      <HugeiconsIcon icon={Share08Icon} />
    </Button>
  )

  if (!requested) return trigger

  return (
    <Suspense fallback={trigger}>
      <ArtifactShareMenu
        artifact={artifact}
        open={open}
        onOpenChange={setOpen}
      />
    </Suspense>
  )
}

function ArtifactRow({
  artifact,
  isDownloadPending,
  onDownload,
  canManageShareLinks,
}: {
  artifact: Artifact
  isDownloadPending: boolean
  onDownload: (artifactId: string, name: string) => void
  canManageShareLinks: boolean
}) {
  const expired = isArtifactExpired(artifact)
  const expiryLabel = artifactExpiryLabel(artifact)
  const installReady = artifactInstallReadiness(artifact).ready

  return (
    <div
      className={`flex items-center gap-2 border p-2 ${expired ? 'opacity-50' : ''}`}
    >
      <div className="min-w-0 flex-1">
        <p className="truncate text-xs font-medium">{artifact.name}</p>
        <div className="mt-0.5 flex items-center gap-1.5">
          <Badge
            variant={artifactTypeBadgeVariant(artifact.artifact_type)}
            className="text-[10px]"
          >
            {artifact.artifact_type}
          </Badge>
          <span className="text-[10px] text-muted-foreground">
            {artifact.file_size != null
              ? formatFileSize(artifact.file_size)
              : '—'}
          </span>
          {expiryLabel ? (
            <span
              className={`text-[10px] ${expired ? 'text-destructive' : 'text-muted-foreground'}`}
            >
              {expiryLabel}
            </span>
          ) : null}
        </div>
      </div>
      <div className="flex shrink-0 items-center gap-1">
        {installReady ? (
          <Button
            variant="outline"
            size="icon-xs"
            render={
              <Link
                to="/builds/$buildId"
                params={{
                  buildId: artifact.build_id,
                }}
                search={{ install: artifact.id }}
              />
            }
            nativeButton={false}
            disabled={expired}
            aria-label={`Install ${artifact.name}`}
            title="Install"
          >
            <HugeiconsIcon icon={Download04Icon} />
          </Button>
        ) : (
          <Button
            variant="outline"
            size="icon-xs"
            title="Download"
            aria-label={`Download ${artifact.name}`}
            onClick={() => onDownload(artifact.id, artifact.name)}
            disabled={isDownloadPending || expired}
          >
            {isDownloadPending ? (
              <Spinner />
            ) : (
              <HugeiconsIcon icon={Download04Icon} />
            )}
          </Button>
        )}
        {canManageShareLinks ? (
          <ArtifactShareControl artifact={artifact} />
        ) : null}
      </div>
    </div>
  )
}

export function ArtifactsPanel({
  artifacts,
  isLoading,
  buildStatus,
  canManageShareLinks,
}: {
  artifacts: Array<Artifact>
  isLoading: boolean
  buildStatus: BuildStatus
  canManageShareLinks: boolean
}) {
  const downloadMutation = useArtifactDownloadLink()

  function handleDownload(artifactId: string, name: string) {
    downloadMutation.mutate(artifactId, {
      onSuccess: (res) => {
        window.open(res.download_url, '_blank', 'noopener,noreferrer')
      },
      onError: (err) => {
        toast.error(`Failed to get download link for ${name}: ${err.message}`)
      },
    })
  }

  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle className="flex items-center gap-2 text-sm font-medium uppercase tracking-wider text-muted-foreground">
          <HugeiconsIcon icon={File01Icon} size={14} />
          Artifacts
          {artifacts.length > 0 ? (
            <Badge variant="secondary" className="text-[10px]">
              {artifacts.length}
            </Badge>
          ) : null}
        </CardTitle>
      </CardHeader>
      <CardContent>
        {isLoading ? (
          <div className="space-y-2">
            <Skeleton className="h-8 w-full" />
            <Skeleton className="h-8 w-full" />
          </div>
        ) : !artifacts.length ? (
          <p className="text-xs text-muted-foreground">
            {artifactEmptyMessage(buildStatus)}
          </p>
        ) : (
          <div className="space-y-2">
            {artifacts.map((artifact) => (
              <ArtifactRow
                key={artifact.id}
                artifact={artifact}
                isDownloadPending={downloadMutation.isPending}
                onDownload={handleDownload}
                canManageShareLinks={canManageShareLinks}
              />
            ))}
          </div>
        )}
      </CardContent>
    </Card>
  )
}
