import { useState } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import { Copy01Icon, Link01Icon } from '@hugeicons/core-free-icons'
import type { BuildPlatform, LatestInstallLink } from '@oore/client/models'

import { QrCode } from '@/components/qr-code'
import { Button } from '@/components/ui/button'
import { Card, CardContent } from '@/components/ui/card'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Row } from '@/components/layout/row'
import { Stack } from '@/components/layout/stack'
import { Text } from '@/components/layout/text'
import {
  useCreateLatestInstallLink,
  useDeleteLatestInstallLink,
  useLatestInstallLinks,
} from '@/hooks/use-testers'
import { getApiErrorMessage } from '@/lib/api-client/api-error'
import { BUILD_PLATFORM_LABELS } from '@/lib/build-platforms'
import { relativeTime } from '@/lib/format-utils'
import { toast } from '@/lib/toast'

function linkTitle(link: LatestInstallLink) {
  return `Newest ${BUILD_PLATFORM_LABELS[link.platform]} build${link.branch ? ` on ${link.branch}` : ''}`
}

function linkUsage(link: LatestInstallLink) {
  const made = `Made ${relativeTime(link.created_at)}${link.created_by_email ? ` by ${link.created_by_email}` : ''}`
  if (!link.use_count) return `${made} · not opened yet`
  return `${made} · opened ${link.use_count} ${link.use_count === 1 ? 'time' : 'times'}${link.last_used_at ? `, last ${relativeTime(link.last_used_at)}` : ''}`
}

function CreatedLinkDialog({
  link,
  onOpenChange,
}: {
  link: LatestInstallLink
  onOpenChange: (open: boolean) => void
}) {
  const url = link.url ?? ''
  return (
    <Dialog open onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{linkTitle(link)}</DialogTitle>
          <DialogDescription>
            This link always opens the newest successful build, so testers can
            bookmark it or print the code. Copy it now: Oore only shows it once.
          </DialogDescription>
        </DialogHeader>
        <Stack gap={3} className="items-center">
          <QrCode
            value={url}
            label="Scan to open the newest build"
            className="size-44"
          />
          <Input
            readOnly
            value={url}
            aria-label="Link"
            className="font-mono text-xs"
          />
        </Stack>
        <DialogFooter>
          <Button
            onClick={() =>
              void navigator.clipboard
                .writeText(url)
                .then(() => toast.success('Link copied'))
            }
          >
            <HugeiconsIcon icon={Copy01Icon} size={14} />
            Copy link
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

function NewLinkDialog({
  pipelineId,
  platforms,
  onCreated,
  onOpenChange,
}: {
  pipelineId: string
  platforms: Array<BuildPlatform>
  onCreated: (link: LatestInstallLink) => void
  onOpenChange: (open: boolean) => void
}) {
  const [platform, setPlatform] = useState<BuildPlatform>(platforms[0])
  const [branch, setBranch] = useState('')
  const create = useCreateLatestInstallLink(pipelineId)
  return (
    <Dialog open onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>New latest-build link</DialogTitle>
          <DialogDescription>
            One permanent link to the newest successful build. Anyone with the
            link can install it, so share it only with your testers.
          </DialogDescription>
        </DialogHeader>
        <Stack gap={4}>
          {platforms.length > 1 ? (
            <Stack gap={2}>
              <Label>Platform</Label>
              <Row gap={2}>
                {platforms.map((value) => (
                  <Button
                    key={value}
                    size="sm"
                    variant={platform === value ? 'default' : 'outline'}
                    aria-pressed={platform === value}
                    onClick={() => setPlatform(value)}
                  >
                    {BUILD_PLATFORM_LABELS[value]}
                  </Button>
                ))}
              </Row>
            </Stack>
          ) : null}
          <Stack gap={2}>
            <Label htmlFor="latest-link-branch">Branch (optional)</Label>
            <Input
              id="latest-link-branch"
              value={branch}
              placeholder="Any branch"
              onChange={(event) => setBranch(event.target.value)}
            />
          </Stack>
        </Stack>
        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button
            disabled={create.isPending}
            onClick={() =>
              create.mutate(
                { platform, branch: branch.trim() || null },
                {
                  onSuccess: onCreated,
                  onError: (error) =>
                    toast.error(getApiErrorMessage(error, {})),
                },
              )
            }
          >
            {create.isPending ? 'Making...' : 'Make link'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

/**
 * Permanent links that always install the newest build of a pipeline, for
 * testers who should not need a new link for every build.
 */
export function LatestInstallLinksCard({
  pipelineId,
  platforms,
  canWrite,
}: {
  pipelineId: string
  platforms: Array<BuildPlatform>
  canWrite: boolean
}) {
  const linksQuery = useLatestInstallLinks(pipelineId)
  const links = linksQuery.data?.links ?? []
  const remove = useDeleteLatestInstallLink(pipelineId)
  const [newOpen, setNewOpen] = useState(false)
  const [created, setCreated] = useState<LatestInstallLink | null>(null)
  if (!platforms.length || (!canWrite && links.length === 0)) return null

  return (
    <Card>
      <CardContent>
        <Row gap={3} className="justify-between pb-3">
          <div className="min-w-0">
            <h3 className="text-sm font-medium">Latest-build links</h3>
            <Text variant="caption">
              One link that always installs the newest build.
            </Text>
          </div>
          {canWrite ? (
            <Button
              size="sm"
              variant="outline"
              onClick={() => setNewOpen(true)}
            >
              <HugeiconsIcon icon={Link01Icon} size={14} />
              New link
            </Button>
          ) : null}
        </Row>
        {links.length ? (
          <ul className="divide-y">
            {links.map((link) => (
              <li
                key={link.id}
                className="flex items-center justify-between gap-3 py-2"
              >
                <div className="min-w-0">
                  <p className="truncate text-sm font-medium">
                    {linkTitle(link)}
                  </p>
                  <Text variant="caption" className="truncate">
                    <span className="font-mono">{link.prefix}…</span> ·{' '}
                    {linkUsage(link)}
                  </Text>
                </div>
                {canWrite ? (
                  <Button
                    size="sm"
                    variant="ghost"
                    disabled={remove.isPending}
                    onClick={() =>
                      remove.mutate(link.id, {
                        onSuccess: () =>
                          toast.success('Link turned off. It no longer opens.'),
                        onError: (error) =>
                          toast.error(getApiErrorMessage(error, {})),
                      })
                    }
                  >
                    Turn off
                  </Button>
                ) : null}
              </li>
            ))}
          </ul>
        ) : (
          <Text variant="muted">
            No links yet. Make one for testers who install every build.
          </Text>
        )}
      </CardContent>
      {newOpen ? (
        <NewLinkDialog
          pipelineId={pipelineId}
          platforms={platforms}
          onOpenChange={setNewOpen}
          onCreated={(link) => {
            setNewOpen(false)
            setCreated(link)
          }}
        />
      ) : null}
      {created ? (
        <CreatedLinkDialog
          link={created}
          onOpenChange={() => setCreated(null)}
        />
      ) : null}
    </Card>
  )
}
