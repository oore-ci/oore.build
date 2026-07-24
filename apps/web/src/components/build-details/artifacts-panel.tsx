import { Suspense, lazy, useState } from 'react'
import { Link } from '@tanstack/react-router'
import {
  Download as Download04Icon,
  File as File01Icon,
  Share2 as Share08Icon,
} from 'lucide-react'
import { toast } from '@/lib/toast'

import type { Artifact, BuildStatus } from '@/lib/types'
import { useArtifactDownloadLink } from '@/hooks/use-builds'
import { formatFileSize, relativeTime } from '@/lib/format-utils'
import { artifactInstallReadiness } from '@/lib/artifact-install'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Skeleton } from '@/components/ui/skeleton'
import { Spinner } from '@/components/ui/spinner'
import {
  Item,
  ItemActions,
  ItemContent,
  ItemDescription,
  ItemTitle,
} from '@/components/ui/item'

const loadArtifactShareMenu = () => import('./artifact-share-menu')
const ArtifactShareMenu = lazy(loadArtifactShareMenu)

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
      <Share08Icon />
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
    <Item
      variant="outline"
      size="xs"
      className={expired ? 'opacity-50' : undefined}
    >
      <ItemContent>
        <ItemTitle>{artifact.name}</ItemTitle>
        <ItemDescription className="flex items-center gap-1.5">
          <Badge variant="outline" className="text-[10px]">
            {artifact.artifact_type}
          </Badge>
          <span>
            {artifact.file_size != null
              ? formatFileSize(artifact.file_size)
              : '—'}
          </span>
          {expiryLabel ? (
            <span className={expired ? 'text-destructive' : undefined}>
              {expiryLabel}
            </span>
          ) : null}
        </ItemDescription>
      </ItemContent>
      <ItemActions>
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
            <Download04Icon />
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
            {isDownloadPending ? <Spinner /> : <Download04Icon />}
          </Button>
        )}
        {canManageShareLinks ? (
          <ArtifactShareControl artifact={artifact} />
        ) : null}
      </ItemActions>
    </Item>
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
        <CardTitle className="flex items-center gap-2">
          <File01Icon size={14} />
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
