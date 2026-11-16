import { useState } from 'react'

import type { ProjectEnvironment, Release } from '@oore/client/models'
import { getApiErrorMessage } from '@/lib/api-client/api-error'
import {
  environmentTargetLabel,
  nextEnvironment,
  rolloutPolicySummary,
} from '@/lib/environments'
import { releaseVersionLabel } from '@/lib/releases'
import { toast } from '@/lib/toast'
import {
  usePromoteBuild,
  usePromotionReadiness,
} from '@/hooks/use-environments'
import { StoreReadinessChecklist } from '@/components/trigger-build-store-section'
import { useReleaseNotes } from '@/hooks/use-release-notes'
import {
  ReleaseNotesDialog,
  releaseNotesSourceLabel,
} from '@/components/release-notes/release-notes-card'
import { Stack } from '@/components/layout/stack'
import { Text } from '@/components/layout/text'
import { Button } from '@/components/ui/button'
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
import { Spinner } from '@/components/ui/spinner'
import { Textarea } from '@/components/ui/textarea'

const PROMOTION_ERRORS = {
  already_in_environment: 'This build already has a release there.',
  promotion_pending:
    'A promotion of this build there is already waiting for approval.',
}

/**
 * Promote the build behind `release` from `source` into another environment.
 * The daemon reuses the version the store already has; nothing is rebuilt.
 */
export function PromoteDialog({
  canEditNotes = false,
  canPromoteDirectly,
  environments,
  onOpenChange,
  open,
  projectId,
  release,
  source,
}: {
  /** Whether this viewer may change the version's release notes. */
  canEditNotes?: boolean
  /** Maintainers promote straight in; others can only ask where approval is required. */
  canPromoteDirectly: boolean
  environments: ReadonlyArray<ProjectEnvironment>
  onOpenChange: (open: boolean) => void
  open: boolean
  projectId: string
  release: Release
  source: ProjectEnvironment
}) {
  const targets = environments.filter(
    (environment) =>
      environment.id !== source.id &&
      environment.promotable &&
      environment.store === release.store &&
      (canPromoteDirectly || environment.approval_required),
  )
  const suggested = nextEnvironment(environments, source)
  const defaultTarget =
    targets.find((environment) => environment.id === suggested?.id) ??
    targets[0]
  const [targetId, setTargetId] = useState<string | undefined>()
  const [reason, setReason] = useState('')
  const [editingNotes, setEditingNotes] = useState(false)
  const notes = useReleaseNotes(projectId, open ? release.version_name : null)
  const promote = usePromoteBuild(projectId)
  const target =
    targets.find((environment) => environment.id === targetId) ?? defaultTarget
  // A TestFlight build going to the App Store gets the same checks an App
  // Store upload runs, before anything is handed to Apple.
  const toAppStore =
    !!target && target.store === 'app_store' && target.track === 'app_store'
  const readiness = usePromotionReadiness(
    projectId,
    target?.id,
    release.build_id,
    { enabled: open && toAppStore },
  )
  const build = release.build_number
    ? `build #${release.build_number}`
    : 'this build'

  function handleOpenChange(next: boolean) {
    if (!next) {
      setTargetId(undefined)
      setReason('')
      promote.reset()
    }
    onOpenChange(next)
  }

  function submit() {
    // Promotion reuses a build Oore uploaded; a release found in the store
    // has none.
    if (!target || !release.build_id) return
    promote.mutate(
      {
        build_id: release.build_id,
        environment_id: target.id,
        reason: reason.trim() || undefined,
      },
      {
        onSuccess: (promotion) => {
          toast.success(
            promotion.state === 'pending'
              ? `Asked to promote ${build} to ${target.name}. A maintainer needs to approve it.`
              : `Promoted ${build} to ${target.name}`,
          )
          handleOpenChange(false)
        },
      },
    )
  }

  const items = Object.fromEntries(
    targets.map((environment) => [environment.id, environment.name]),
  )

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Promote {build}</DialogTitle>
          <DialogDescription>
            {releaseVersionLabel(release)} from {source.name}. The store gets
            the version it already has; nothing is rebuilt.
          </DialogDescription>
        </DialogHeader>

        <Stack gap={4}>
          <Stack gap={2}>
            <Label htmlFor="promote-target">Promote to</Label>
            <Select
              value={target?.id ?? null}
              onValueChange={(next) => setTargetId(next ?? undefined)}
              items={items}
            >
              <SelectTrigger id="promote-target" className="w-full">
                <SelectValue placeholder="No environment to promote to" />
              </SelectTrigger>
              <SelectContent>
                {targets.map((environment) => (
                  <SelectItem key={environment.id} value={environment.id}>
                    {environment.name}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            {target ? (
              <Text>
                {environmentTargetLabel(target)}. {rolloutPolicySummary(target)}
                .
                {target.approval_required
                  ? ' Waits for a maintainer to approve before the store changes.'
                  : ''}
              </Text>
            ) : null}
          </Stack>

          {toAppStore ? (
            <StoreReadinessChecklist
              readiness={readiness}
              title="Ready for the App Store?"
              blockingNote="You can still promote, but App Store Connect will refuse the submission until these are fixed."
            />
          ) : null}

          {release.version_name ? (
            <Stack gap={2}>
              <div className="flex items-center justify-between gap-2">
                <Label>Release notes</Label>
                {canEditNotes && notes.data?.source !== 'repository' ? (
                  <Button
                    type="button"
                    variant="ghost"
                    size="xs"
                    onClick={() => setEditingNotes(true)}
                  >
                    {notes.data?.text ? 'Edit' : 'Write notes'}
                  </Button>
                ) : null}
              </div>
              {notes.data?.text ? (
                <>
                  <p className="line-clamp-4 text-sm whitespace-pre-line">
                    {notes.data.text}
                  </p>
                  <Text variant="muted" className="text-xs">
                    {releaseNotesSourceLabel(notes.data)}. The store gets these
                    with the promotion.
                  </Text>
                </>
              ) : (
                <Text variant="muted">
                  {notes.isLoading
                    ? 'Loading the notes...'
                    : `No notes for ${release.version_name}. The store keeps what it already shows.`}
                </Text>
              )}
            </Stack>
          ) : null}

          <Stack gap={2}>
            <Label htmlFor="promote-reason">Reason</Label>
            <Textarea
              id="promote-reason"
              rows={2}
              maxLength={500}
              placeholder="QA signed off on 2.4.0"
              value={reason}
              onChange={(event) => setReason(event.target.value)}
            />
          </Stack>

          {promote.error ? (
            <p role="alert" className="text-sm text-destructive">
              {getApiErrorMessage(promote.error, PROMOTION_ERRORS)}
            </p>
          ) : null}
        </Stack>

        <DialogFooter>
          <Button
            type="button"
            variant="outline"
            onClick={() => handleOpenChange(false)}
          >
            Cancel
          </Button>
          <Button
            type="button"
            disabled={!target || promote.isPending}
            onClick={submit}
          >
            {promote.isPending ? <Spinner data-icon="inline-start" /> : null}
            {target?.approval_required ? 'Ask to promote' : 'Promote'}
          </Button>
        </DialogFooter>
      </DialogContent>
      {editingNotes && release.version_name ? (
        <ReleaseNotesDialog
          projectId={projectId}
          versionName={release.version_name}
          notes={notes.data}
          onOpenChange={setEditingNotes}
        />
      ) : null}
    </Dialog>
  )
}
