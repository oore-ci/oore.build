import { useState } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import { SnowIcon } from '@hugeicons/core-free-icons'
import type { ReleaseFreeze } from '@oore/client/models'

import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
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
import { Label } from '@/components/ui/label'
import { Textarea } from '@/components/ui/textarea'
import { Row } from '@/components/layout/row'
import { Stack } from '@/components/layout/stack'
import {
  useActiveFreeze,
  useLiftReleaseFreeze,
  useSetReleaseFreeze,
} from '@/hooks/use-release-freezes'
import { getApiErrorMessage } from '@/lib/api-client/api-error'
import { relativeTime } from '@/lib/format-utils'
import { toast } from '@/lib/toast'
import { useAuthStore } from '@/stores/auth-store'

function useIsInstanceAdmin() {
  const role = useAuthStore((s) => s.user?.role)
  return role === 'owner' || role === 'admin'
}

/** What a freeze holds, in one sentence everyone sees. */
export const FREEZE_EFFECT =
  'Store uploads are skipped, and promotions, approvals and rollout increases wait. Pausing and halting still work.'

function freezeTitle(freeze: ReleaseFreeze) {
  return freeze.project_id
    ? 'Releases of this app are frozen'
    : 'Releases of every app are frozen'
}

export function ReleaseFreezeBanner({
  freeze,
  canLift,
}: {
  freeze: ReleaseFreeze
  canLift: boolean
}) {
  const lift = useLiftReleaseFreeze()
  return (
    <Alert>
      <HugeiconsIcon icon={SnowIcon} size={16} />
      <AlertTitle>{freezeTitle(freeze)}</AlertTitle>
      <AlertDescription className="flex flex-col gap-2">
        <span>
          {freeze.reason}. {FREEZE_EFFECT}
        </span>
        <Row gap={3} className="flex-wrap justify-between">
          <span className="text-xs">
            Frozen {relativeTime(freeze.created_at)}
            {freeze.created_by_email ? ` by ${freeze.created_by_email}` : ''}
          </span>
          {canLift ? (
            <Button
              size="sm"
              variant="outline"
              disabled={lift.isPending}
              onClick={() =>
                lift.mutate(freeze.id, {
                  onSuccess: () =>
                    toast.success('Freeze lifted. Releases go ahead again.'),
                  onError: (error) =>
                    toast.error(getApiErrorMessage(error, {})),
                })
              }
            >
              {lift.isPending ? 'Lifting...' : 'Lift freeze'}
            </Button>
          ) : null}
        </Row>
      </AlertDescription>
    </Alert>
  )
}

function FreezeDialog({
  projectId,
  onOpenChange,
}: {
  projectId: string
  onOpenChange: (open: boolean) => void
}) {
  const isAdmin = useIsInstanceAdmin()
  const [reason, setReason] = useState('')
  const [everyApp, setEveryApp] = useState(false)
  const freeze = useSetReleaseFreeze()
  return (
    <Dialog open onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Freeze releases</DialogTitle>
          <DialogDescription>
            {FREEZE_EFFECT} Builds still run and testers can still install them.
          </DialogDescription>
        </DialogHeader>
        <Stack gap={4}>
          <Stack gap={2}>
            <Label htmlFor="freeze-reason">Why</Label>
            <Textarea
              id="freeze-reason"
              value={reason}
              maxLength={500}
              placeholder="Sale weekend, no changes until Monday"
              onChange={(event) => setReason(event.target.value)}
            />
          </Stack>
          {isAdmin ? (
            <Row gap={2}>
              <Checkbox
                id="freeze-every-app"
                checked={everyApp}
                onCheckedChange={(checked) => setEveryApp(checked)}
              />
              <Label htmlFor="freeze-every-app">Freeze every app</Label>
            </Row>
          ) : null}
        </Stack>
        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button
            disabled={!reason.trim() || freeze.isPending}
            onClick={() =>
              freeze.mutate(
                {
                  project_id: everyApp ? null : projectId,
                  reason: reason.trim(),
                },
                {
                  onSuccess: () => {
                    toast.success(
                      everyApp ? 'Every app is frozen' : 'This app is frozen',
                    )
                    onOpenChange(false)
                  },
                  onError: (error) =>
                    toast.error(getApiErrorMessage(error, {})),
                },
              )
            }
          >
            {freeze.isPending ? 'Freezing...' : 'Freeze'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

/**
 * The freeze on a release's own page, so nobody tries a raise or promotion
 * that the daemon will refuse. Lifting stays on the app's Releases tab.
 */
export function ReleaseFreezeNotice({ projectId }: { projectId: string }) {
  const freeze = useActiveFreeze(projectId)
  return freeze ? <ReleaseFreezeBanner freeze={freeze} canLift={false} /> : null
}

/** The app's freeze, with Freeze or Lift for people who manage releases. */
export function ReleaseFreezeControl({
  projectId,
  canManage,
}: {
  projectId: string
  canManage: boolean
}) {
  const freeze = useActiveFreeze(projectId)
  const isAdmin = useIsInstanceAdmin()
  const [open, setOpen] = useState(false)
  if (freeze) {
    return (
      <ReleaseFreezeBanner
        freeze={freeze}
        canLift={freeze.project_id ? canManage : isAdmin}
      />
    )
  }
  if (!canManage) return null
  return (
    <>
      <Button size="sm" variant="outline" onClick={() => setOpen(true)}>
        <HugeiconsIcon icon={SnowIcon} size={14} />
        Freeze releases
      </Button>
      {open ? (
        <FreezeDialog projectId={projectId} onOpenChange={setOpen} />
      ) : null}
    </>
  )
}
