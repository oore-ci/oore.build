import { useState } from 'react'
import { Copy as Copy01Icon, Share2 as Share08Icon } from 'lucide-react'
import { toast } from '@/lib/toast'

import type { Artifact, CreateScopedDownloadTokenResponse } from '@/lib/types'
import {
  useArtifactDownloadLink,
  useCreateScopedDownloadToken,
} from '@/hooks/use-builds'
import { relativeTime } from '@/lib/format-utils'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { Label } from '@/components/ui/label'
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Spinner } from '@/components/ui/spinner'

const TTL_OPTIONS = [
  { value: '3600', label: '1 hour' },
  { value: '21600', label: '6 hours' },
  { value: '86400', label: '24 hours' },
  { value: '604800', label: '7 days' },
] as const

export default function ArtifactShareMenu({
  artifact,
  open,
  onOpenChange,
}: {
  artifact: Artifact
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  const downloadMutation = useArtifactDownloadLink()
  const createTokenMutation = useCreateScopedDownloadToken()
  const [dialogOpen, setDialogOpen] = useState(false)
  const [ttlSecs, setTtlSecs] = useState('86400')
  const [singleUse, setSingleUse] = useState(false)
  const [createdToken, setCreatedToken] =
    useState<CreateScopedDownloadTokenResponse | null>(null)

  function copyDownloadLink() {
    downloadMutation.mutate(artifact.id, {
      onSuccess: (response) => {
        void navigator.clipboard.writeText(response.download_url).then(
          () => toast.success(`Download link copied for ${artifact.name}`),
          () => toast.error('Failed to copy link'),
        )
      },
      onError: (error) => {
        toast.error(`Failed to get link for ${artifact.name}: ${error.message}`)
      },
    })
  }

  function openShareDialog() {
    setCreatedToken(null)
    setTtlSecs('86400')
    setSingleUse(false)
    setDialogOpen(true)
  }

  function createShareLink() {
    createTokenMutation.mutate(
      {
        artifactId: artifact.id,
        data: { ttl_secs: Number(ttlSecs), single_use: singleUse },
      },
      {
        onSuccess: setCreatedToken,
        onError: (error) => {
          toast.error(`Failed to create share link: ${error.message}`)
        },
      },
    )
  }

  function copyShareUrl() {
    if (!createdToken) return
    void navigator.clipboard.writeText(createdToken.download_url).then(
      () => toast.success('Share link copied to clipboard'),
      () => toast.error('Failed to copy link'),
    )
  }

  return (
    <>
      <DropdownMenu open={open} onOpenChange={onOpenChange}>
        <DropdownMenuTrigger
          render={
            <Button
              variant="outline"
              size="icon-xs"
              aria-label={`Share options for ${artifact.name}`}
              title="Share options"
            />
          }
        >
          <Share08Icon />
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="w-auto">
          <DropdownMenuGroup>
            <DropdownMenuItem
              onClick={copyDownloadLink}
              disabled={downloadMutation.isPending}
            >
              <Copy01Icon />
              Copy download link
            </DropdownMenuItem>
            <DropdownMenuItem onClick={openShareDialog}>
              <Share08Icon />
              Create share link
            </DropdownMenuItem>
          </DropdownMenuGroup>
        </DropdownMenuContent>
      </DropdownMenu>

      <Dialog open={dialogOpen} onOpenChange={setDialogOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>
              {createdToken ? 'Share Link Created' : 'Create Share Link'}
            </DialogTitle>
            <DialogDescription>
              {createdToken
                ? 'Copy this link to share. It will not be shown again.'
                : `Generate a scoped download link for "${artifact.name}".`}
            </DialogDescription>
          </DialogHeader>

          {createdToken ? (
            <div className="space-y-3">
              <Alert>
                <AlertDescription className="font-mono text-xs break-all">
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
                  onClick={() => setDialogOpen(false)}
                >
                  Close
                </Button>
                <Button onClick={copyShareUrl}>
                  <Copy01Icon size={14} className="mr-1.5" />
                  Copy link
                </Button>
              </DialogFooter>
            </div>
          ) : (
            <div className="flex flex-col gap-4">
              <div className="flex flex-col gap-2">
                <Label htmlFor="ttl-select">Expires after</Label>
                <Select
                  items={TTL_OPTIONS}
                  value={ttlSecs}
                  onValueChange={(value) => {
                    if (value != null) setTtlSecs(value)
                  }}
                >
                  <SelectTrigger id="ttl-select">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectGroup>
                      {TTL_OPTIONS.map((option) => (
                        <SelectItem key={option.value} value={option.value}>
                          {option.label}
                        </SelectItem>
                      ))}
                    </SelectGroup>
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
                  onClick={() => setDialogOpen(false)}
                >
                  Cancel
                </Button>
                <Button
                  onClick={createShareLink}
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
