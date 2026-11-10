import { useState } from 'react'
import { useNavigate } from '@tanstack/react-router'
import type { Release } from '@oore/client/models'

import { Button } from '@/components/ui/button'
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
import { Textarea } from '@/components/ui/textarea'
import { Row } from '@/components/layout/row'
import { Stack } from '@/components/layout/stack'
import { Text } from '@/components/layout/text'
import { useBuild, useCreateBuild, useRerunBuild } from '@/hooks/use-builds'
import { useReleases } from '@/hooks/use-releases'
import { getApiErrorMessage } from '@/lib/api-client/api-error'
import { formatRolloutPercent, releaseVersionLabel } from '@/lib/releases'
import { toast } from '@/lib/toast'

/** Releases a hotfix can follow: out, or on the way out. */
export function canShipHotfix(release: Release) {
  return (
    !!release.build_id &&
    ['rolling', 'paused', 'halted', 'complete'].includes(release.state)
  )
}

/** The version that was out on the same track before this one. */
export function previousRelease(
  release: Release,
  releases: ReadonlyArray<Release>,
): Release | null {
  return (
    releases
      .filter(
        (candidate) =>
          candidate.id !== release.id &&
          candidate.store === release.store &&
          candidate.track === release.track &&
          candidate.build_id &&
          candidate.build_id !== release.build_id &&
          candidate.created_at < release.created_at &&
          ['complete', 'rolling', 'paused'].includes(candidate.state),
      )
      .sort((a, b) => b.created_at - a.created_at)[0] ?? null
  )
}

function HotfixDialog({
  release,
  onOpenChange,
}: {
  release: Release
  onOpenChange: (open: boolean) => void
}) {
  const buildQuery = useBuild(release.build_id ?? '')
  const build = buildQuery.data?.build
  const version = release.version_name ?? 'release'
  const [branch, setBranch] = useState(`hotfix/${version}`)
  const [notes, setNotes] = useState('')
  const create = useCreateBuild()
  const navigate = useNavigate()
  const commit = build?.commit_sha?.slice(0, 7)

  function start() {
    if (!build) return
    create.mutate(
      {
        projectId: release.project_id,
        data: {
          pipeline_id: build.pipeline_id,
          branch: branch.trim(),
          workflow: build.workflow ?? null,
          store_notes: notes.trim() || null,
        },
      },
      {
        onSuccess: (response) => {
          toast.success(`Hotfix build #${response.build.build_number} started`)
          onOpenChange(false)
          void navigate({
            to: '/builds/$buildId',
            params: { buildId: response.build.id },
          })
        },
        onError: (error) => toast.error(getApiErrorMessage(error, {})),
      },
    )
  }

  return (
    <Dialog open onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Ship a hotfix for {version}</DialogTitle>
          <DialogDescription>
            A hotfix is a new build with only the fix, released the same way as{' '}
            {version}.
          </DialogDescription>
        </DialogHeader>
        <ol className="flex list-decimal flex-col gap-1.5 pl-5 text-sm">
          <li>
            Make a branch from{' '}
            {commit ? (
              <span className="font-mono text-xs">{commit}</span>
            ) : (
              'the commit this build was made from'
            )}
            , the commit {version} was built from, and push the fix to it.
          </li>
          <li>Build that branch here. It gets a higher version code.</li>
          <li>
            When it finishes, release it from its build page. The
            pipeline&apos;s publish settings send it to the same track.
          </li>
        </ol>
        <Stack gap={4}>
          <Stack gap={2}>
            <Label htmlFor="hotfix-branch">Branch with the fix</Label>
            <Input
              id="hotfix-branch"
              value={branch}
              onChange={(event) => setBranch(event.target.value)}
              className="font-mono text-xs"
            />
          </Stack>
          <Stack gap={2}>
            <Label htmlFor="hotfix-notes">
              What&apos;s new, for the stores (optional)
            </Label>
            <Textarea
              id="hotfix-notes"
              value={notes}
              maxLength={4000}
              placeholder="Fixes a crash when opening the cart."
              onChange={(event) => setNotes(event.target.value)}
            />
          </Stack>
        </Stack>
        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            Not yet
          </Button>
          <Button
            disabled={!build || !branch.trim() || create.isPending}
            onClick={start}
          >
            {create.isPending ? 'Starting...' : 'Build the branch'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

export function HotfixButton({ release }: { release: Release }) {
  const [open, setOpen] = useState(false)
  if (!canShipHotfix(release)) return null
  return (
    <>
      <Button size="sm" variant="outline" onClick={() => setOpen(true)}>
        Ship a hotfix
      </Button>
      {open ? <HotfixDialog release={release} onOpenChange={setOpen} /> : null}
    </>
  )
}

/**
 * A halted release in plain words: who has it, what a store can and cannot
 * undo, and the ways back.
 */
export function HaltedRecovery({ release }: { release: Release }) {
  const releasesQuery = useReleases(
    { project_id: release.project_id, limit: 200 },
    { enabled: release.state === 'halted' },
  )
  const previous = previousRelease(release, releasesQuery.data?.releases ?? [])
  const rerun = useRerunBuild()
  const navigate = useNavigate()
  if (release.state !== 'halted') return null
  const version = releaseVersionLabel(release)
  const share = formatRolloutPercent(release.rollout_fraction)

  return (
    <section
      aria-label="What the halt means"
      className="flex flex-col gap-3 rounded-lg border p-3 text-sm"
    >
      <p>
        <span className="font-medium">Nobody new gets {version}.</span> The{' '}
        {share} of users who already updated keep it: a store cannot take an
        update back off a phone.
      </p>
      <p className="text-muted-foreground">
        To move those users off it, ship a version with a higher version code:
        either a fix, or{' '}
        {previous
          ? `the code of ${releaseVersionLabel(previous)} built again`
          : 'the previous version built again'}
        . If the problem turned out to be elsewhere, resume the rollout instead.
      </p>
      <Row gap={2} className="flex-wrap">
        <HotfixButton release={release} />
        {previous?.build_id ? (
          <Button
            size="sm"
            variant="outline"
            disabled={rerun.isPending}
            onClick={() =>
              rerun.mutate(previous.build_id ?? '', {
                onSuccess: (response) => {
                  toast.success(
                    `Building ${previous.version_name ?? 'the previous version'} again as #${response.build.build_number}`,
                  )
                  void navigate({
                    to: '/builds/$buildId',
                    params: { buildId: response.build.id },
                  })
                },
                onError: (error) => toast.error(getApiErrorMessage(error, {})),
              })
            }
          >
            Build {previous.version_name ?? 'the previous version'} again
          </Button>
        ) : null}
      </Row>
      {!previous && releasesQuery.isSuccess ? (
        <Text variant="caption">
          No earlier version on this track to build again.
        </Text>
      ) : null}
    </section>
  )
}
