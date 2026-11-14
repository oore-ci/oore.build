import { useState } from 'react'
import { Link } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import { SentIcon } from '@hugeicons/core-free-icons'
import type { BuildShare } from '@oore/client/models'

import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
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
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Textarea } from '@/components/ui/textarea'
import { Row } from '@/components/layout/row'
import { Stack } from '@/components/layout/stack'
import { Text } from '@/components/layout/text'
import {
  useBuildInstalls,
  useBuildShares,
  useSendBuildToGroup,
  useTesterGroups,
} from '@/hooks/use-testers'
import { getApiErrorMessage } from '@/lib/api-client/api-error'
import { relativeTime } from '@/lib/format-utils'
import { toast } from '@/lib/toast'

const VIA_LABELS = new Map([
  ['install_page', 'install page'],
  ['share_link', 'share link'],
  ['latest_link', 'latest-build link'],
])

function shareSummary(share: BuildShare) {
  const reachable = share.recipients.filter((r) => !r.no_access)
  const installed = reachable.filter((r) => r.installed_at).length
  return `${installed} of ${reachable.length} installed`
}

function ShareRow({ share }: { share: BuildShare }) {
  const [open, setOpen] = useState(false)
  const blocked = share.recipients.filter((r) => r.no_access).length
  return (
    <li className="py-2">
      <Row gap={3} className="justify-between">
        <div className="min-w-0">
          <p className="truncate font-medium">{share.group_name}</p>
          <Text variant="caption">
            {relativeTime(share.sent_at)}
            {share.sent_by_email ? ` by ${share.sent_by_email}` : ''}
            {blocked ? ` · ${blocked} without access` : ''}
          </Text>
        </div>
        <Button
          variant="link"
          size="xs"
          className="h-auto shrink-0 px-0"
          aria-expanded={open}
          onClick={() => setOpen((value) => !value)}
        >
          {shareSummary(share)}
        </Button>
      </Row>
      {open ? (
        <ul className="mt-2 space-y-1 text-sm" aria-label="Who it went to">
          {share.recipients.map((recipient) => (
            <li key={recipient.user_id} className="flex justify-between gap-3">
              <span className="min-w-0 truncate">
                {recipient.name ?? recipient.email}
              </span>
              <Text variant="caption" render={<span />}>
                {recipient.no_access
                  ? 'No access to this app'
                  : recipient.installed_at
                    ? `Installed ${relativeTime(recipient.installed_at)}`
                    : 'Not yet'}
              </Text>
            </li>
          ))}
        </ul>
      ) : null}
    </li>
  )
}

function SendToTestersDialog({
  buildId,
  buildNumber,
  open,
  onOpenChange,
}: {
  buildId: string
  buildNumber: number
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  const groupsQuery = useTesterGroups({ enabled: open })
  const groups = groupsQuery.data?.groups ?? []
  const [groupId, setGroupId] = useState<string | null>(null)
  const [message, setMessage] = useState('')
  const send = useSendBuildToGroup(buildId)
  const chosen = groupId ?? groups[0]?.id ?? null
  const items = Object.fromEntries(
    groups.map((group) => [
      group.id,
      `${group.name} (${group.members.length})`,
    ]),
  )

  function submit() {
    if (!chosen) return
    send.mutate(
      { group_id: chosen, message: message.trim() || null },
      {
        onSuccess: (share) => {
          const reached = share.recipients.filter((r) => !r.no_access).length
          toast.success(
            `Build #${buildNumber} sent to ${reached} ${reached === 1 ? 'tester' : 'testers'} in ${share.group_name}`,
          )
          setMessage('')
          onOpenChange(false)
        },
        onError: (error) =>
          toast.error(
            getApiErrorMessage(error, {
              build_not_succeeded: 'Only a finished build can be sent.',
            }),
          ),
      },
    )
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Send build #{buildNumber} to testers</DialogTitle>
          <DialogDescription>
            Each tester gets an email with their own install link, and a browser
            notification if they turned those on.
          </DialogDescription>
        </DialogHeader>
        {groupsQuery.isSuccess && groups.length === 0 ? (
          <Text variant="muted">
            There are no tester groups yet.{' '}
            <Link
              to="/settings/testers"
              className="underline underline-offset-4"
            >
              Make one in Settings
            </Link>
            .
          </Text>
        ) : (
          <Stack gap={4}>
            <Stack gap={2}>
              <Label htmlFor="send-testers-group">Group</Label>
              <Select
                value={chosen}
                onValueChange={(value) => setGroupId(value)}
                items={items}
              >
                <SelectTrigger id="send-testers-group" className="w-full">
                  <SelectValue placeholder="Choose a group" />
                </SelectTrigger>
                <SelectContent>
                  <SelectGroup>
                    {groups.map((group) => (
                      <SelectItem key={group.id} value={group.id}>
                        {items[group.id]}
                      </SelectItem>
                    ))}
                  </SelectGroup>
                </SelectContent>
              </Select>
            </Stack>
            <Stack gap={2}>
              <Label htmlFor="send-testers-message">
                What to test (optional)
              </Label>
              <Textarea
                id="send-testers-message"
                value={message}
                maxLength={1000}
                placeholder="Check the new sign-in screen."
                onChange={(event) => setMessage(event.target.value)}
              />
            </Stack>
          </Stack>
        )}
        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button disabled={!chosen || send.isPending} onClick={submit}>
            {send.isPending ? 'Sending...' : 'Send'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

/**
 * Who this build went to and who installed it, with Send to testers for
 * people who can share the build's files.
 */
export function BuildTestersSection({
  buildId,
  buildNumber,
  canSend,
}: {
  buildId: string
  buildNumber: number
  canSend: boolean
}) {
  const [sendOpen, setSendOpen] = useState(false)
  const sharesQuery = useBuildShares(buildId)
  const installsQuery = useBuildInstalls(buildId)
  const shares = sharesQuery.data?.shares ?? []
  const installs = installsQuery.data?.installs ?? []
  const people = new Set(
    installs.map((install) => install.user_email ?? 'someone with a link'),
  )

  return (
    <Card size="sm">
      <CardHeader>
        <Row gap={3} className="justify-between">
          <CardTitle>Testers</CardTitle>
          {canSend ? (
            <Button
              size="sm"
              variant="outline"
              onClick={() => setSendOpen(true)}
            >
              <HugeiconsIcon icon={SentIcon} size={14} />
              Send to testers
            </Button>
          ) : null}
        </Row>
      </CardHeader>
      <CardContent>
        <Stack gap={3}>
          {shares.length ? (
            <ul className="divide-y" aria-label="Sent to">
              {shares.map((share) => (
                <ShareRow key={share.id} share={share} />
              ))}
            </ul>
          ) : null}
          <Text variant={installs.length ? 'caption' : 'muted'}>
            {installs.length
              ? `Installed ${installs.length} ${installs.length === 1 ? 'time' : 'times'} by ${people.size} ${people.size === 1 ? 'person' : 'people'}, last ${relativeTime(installs[0].installed_at)} through the ${VIA_LABELS.get(installs[0].via) ?? 'install page'}.`
              : shares.length
                ? 'No installs yet.'
                : 'Not sent to testers or installed yet.'}
          </Text>
        </Stack>
      </CardContent>
      {sendOpen ? (
        <SendToTestersDialog
          buildId={buildId}
          buildNumber={buildNumber}
          open={sendOpen}
          onOpenChange={setSendOpen}
        />
      ) : null}
    </Card>
  )
}
