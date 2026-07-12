import { useState } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  Copy01Icon,
  Download04Icon,
  File01Icon,
  Share08Icon,
} from '@hugeicons/core-free-icons'
import { toast } from 'sonner'

import type {
  Artifact,
  BuildStatus,
  CreateScopedDownloadTokenResponse,
} from '@/lib/types'
import {
  useArtifactDownloadLink,
  useCreateScopedDownloadToken,
} from '@/hooks/use-builds'
import { formatFileSize, relativeTime } from '@/lib/format-utils'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Checkbox } from '@/components/ui/checkbox'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Label } from '@/components/ui/label'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Skeleton } from '@/components/ui/skeleton'
import { Spinner } from '@/components/ui/spinner'

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

const TTL_OPTIONS = [
  { value: '3600', label: '1 hour' },
  { value: '21600', label: '6 hours' },
  { value: '86400', label: '24 hours' },
  { value: '604800', label: '7 days' },
] as const

export function ArtifactsPanel({
  artifacts,
  isLoading,
  buildStatus,
}: {
  artifacts: Array<Artifact>
  isLoading: boolean
  buildStatus: BuildStatus
}) {
  const downloadMutation = useArtifactDownloadLink()
  const createTokenMutation = useCreateScopedDownloadToken()

  const [shareArtifact, setShareArtifact] = useState<Artifact | null>(null)
  const [ttlSecs, setTtlSecs] = useState('86400')
  const [singleUse, setSingleUse] = useState(false)
  const [createdToken, setCreatedToken] =
    useState<CreateScopedDownloadTokenResponse | null>(null)

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

  function handleCopyLink(artifactId: string, name: string) {
    downloadMutation.mutate(artifactId, {
      onSuccess: (res) => {
        void navigator.clipboard.writeText(res.download_url).then(
          () => toast.success(`Download link copied for ${name}`),
          () => toast.error('Failed to copy link'),
        )
      },
      onError: (err) => {
        toast.error(`Failed to get link for ${name}: ${err.message}`)
      },
    })
  }

  function handleShareLink(artifact: Artifact) {
    setShareArtifact(artifact)
    setCreatedToken(null)
    setTtlSecs('86400')
    setSingleUse(false)
  }

  function handleCreateToken() {
    if (!shareArtifact) return
    createTokenMutation.mutate(
      {
        artifactId: shareArtifact.id,
        data: {
          ttl_secs: Number(ttlSecs),
          single_use: singleUse,
        },
      },
      {
        onSuccess: (res) => {
          setCreatedToken(res)
        },
        onError: (err) => {
          toast.error(`Failed to create share link: ${err.message}`)
        },
      },
    )
  }

  function handleCopyShareUrl() {
    if (!createdToken) return
    void navigator.clipboard.writeText(createdToken.download_url).then(
      () => toast.success('Share link copied to clipboard'),
      () => toast.error('Failed to copy link'),
    )
  }

  return (
    <>
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
              {artifacts.map((artifact) => {
                const expired = isArtifactExpired(artifact)
                const expiryLabel = artifactExpiryLabel(artifact)

                return (
                  <div
                    key={artifact.id}
                    className={`flex items-center gap-2 border p-2 ${expired ? 'opacity-50' : ''}`}
                  >
                    <div className="min-w-0 flex-1">
                      <p className="truncate text-xs font-medium">
                        {artifact.name}
                      </p>
                      <div className="mt-0.5 flex items-center gap-1.5">
                        <Badge
                          variant={artifactTypeBadgeVariant(
                            artifact.artifact_type,
                          )}
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
                    <div className="flex items-center gap-0.5">
                      <Button
                        variant="ghost"
                        size="icon"
                        className="size-7 shrink-0"
                        title="Share link"
                        aria-label={`Share link for ${artifact.name}`}
                        onClick={() => handleShareLink(artifact)}
                        disabled={expired}
                      >
                        <HugeiconsIcon icon={Share08Icon} size={14} />
                      </Button>
                      <Button
                        variant="ghost"
                        size="icon"
                        className="size-7 shrink-0"
                        title="Copy download link"
                        aria-label={`Copy link for ${artifact.name}`}
                        onClick={() =>
                          handleCopyLink(artifact.id, artifact.name)
                        }
                        disabled={downloadMutation.isPending || expired}
                      >
                        <HugeiconsIcon icon={Copy01Icon} size={14} />
                      </Button>
                      <Button
                        variant="ghost"
                        size="icon"
                        className="size-7 shrink-0"
                        title="Download"
                        aria-label={`Download ${artifact.name}`}
                        onClick={() =>
                          handleDownload(artifact.id, artifact.name)
                        }
                        disabled={downloadMutation.isPending || expired}
                      >
                        <HugeiconsIcon icon={Download04Icon} size={14} />
                      </Button>
                    </div>
                  </div>
                )
              })}
            </div>
          )}
        </CardContent>
      </Card>

      <Dialog
        open={shareArtifact !== null}
        onOpenChange={(open) => {
          if (!open) setShareArtifact(null)
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>
              {createdToken ? 'Share Link Created' : 'Create Share Link'}
            </DialogTitle>
            <DialogDescription>
              {createdToken
                ? 'Copy this link to share. It will not be shown again.'
                : `Generate a scoped download link for "${shareArtifact?.name}".`}
            </DialogDescription>
          </DialogHeader>

          {createdToken ? (
            <div className="space-y-3">
              <Alert>
                <AlertDescription className="break-all text-xs font-mono">
                  {createdToken.download_url}
                </AlertDescription>
              </Alert>
              <div className="flex items-center gap-2 text-xs text-muted-foreground">
                <span>Expires {relativeTime(createdToken.expires_at)}</span>
                {createdToken.single_use ? (
                  <Badge variant="secondary" className="text-[10px]">
                    Single use
                  </Badge>
                ) : null}
              </div>
              <DialogFooter>
                <Button
                  variant="secondary"
                  onClick={() => setShareArtifact(null)}
                >
                  Close
                </Button>
                <Button onClick={handleCopyShareUrl}>
                  <HugeiconsIcon
                    icon={Copy01Icon}
                    size={14}
                    className="mr-1.5"
                  />
                  Copy link
                </Button>
              </DialogFooter>
            </div>
          ) : (
            <div className="space-y-4">
              <div className="space-y-2">
                <Label htmlFor="ttl-select">Expires after</Label>
                <Select
                  value={ttlSecs}
                  onValueChange={(value) => {
                    if (value != null) setTtlSecs(value)
                  }}
                >
                  <SelectTrigger id="ttl-select">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {TTL_OPTIONS.map((option) => (
                      <SelectItem key={option.value} value={option.value}>
                        {option.label}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
              <div className="flex items-center gap-2">
                <Checkbox
                  id="single-use"
                  checked={singleUse}
                  onCheckedChange={(checked) => setSingleUse(checked === true)}
                />
                <Label htmlFor="single-use" className="text-sm font-normal">
                  Single use (consumed after first download)
                </Label>
              </div>
              <DialogFooter>
                <Button
                  variant="secondary"
                  onClick={() => setShareArtifact(null)}
                >
                  Cancel
                </Button>
                <Button
                  onClick={handleCreateToken}
                  disabled={createTokenMutation.isPending}
                >
                  {createTokenMutation.isPending ? (
                    <>
                      <Spinner className="mr-1.5" />
                      Creating...
                    </>
                  ) : (
                    'Create Link'
                  )}
                </Button>
              </DialogFooter>
            </div>
          )}
        </DialogContent>
      </Dialog>
    </>
  )
}
