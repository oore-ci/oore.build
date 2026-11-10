import { useState } from 'react'
import { createLazyFileRoute } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import { Add01Icon, UserGroupIcon } from '@hugeicons/core-free-icons'
import type { TesterGroup, TesterGroupMember } from '@oore/client/models'

import PageHeader from '@/components/page-header'
import PageLayout from '@/components/page-layout'
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
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from '@/components/ui/empty'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Row } from '@/components/layout/row'
import { Stack } from '@/components/layout/stack'
import { Text } from '@/components/layout/text'
import { useUsers } from '@/hooks/use-auth'
import {
  useDeleteTesterGroup,
  useSaveTesterGroup,
  useTesterGroups,
} from '@/hooks/use-testers'
import { getApiErrorMessage } from '@/lib/api-client/api-error'
import { PageMeta } from '@/lib/seo'
import { toast } from '@/lib/toast'

export const Route = createLazyFileRoute('/settings/testers')({
  component: TesterGroupsPage,
})

function memberLabel(member: TesterGroupMember) {
  return member.name ? `${member.name} (${member.email})` : member.email
}

function GroupDialog({
  group,
  onOpenChange,
}: {
  group: TesterGroup | null
  onOpenChange: (open: boolean) => void
}) {
  const [name, setName] = useState(group?.name ?? '')
  const [search, setSearch] = useState('')
  const [members, setMembers] = useState(
    () =>
      new Map(
        (group?.members ?? []).map((member) => [member.user_id, member.email]),
      ),
  )
  const usersQuery = useUsers({ q: search.trim() || undefined, limit: 100 })
  const users = (usersQuery.data?.users ?? []).filter(
    (user) => user.status === 'active',
  )
  const save = useSaveTesterGroup()

  function toggle(userId: string, email: string, checked: boolean) {
    setMembers((current) => {
      const next = new Map(current)
      if (checked) next.set(userId, email)
      else next.delete(userId)
      return next
    })
  }

  function submit() {
    save.mutate(
      {
        id: group?.id,
        body: { name: name.trim(), member_user_ids: [...members.keys()] },
      },
      {
        onSuccess: (saved) => {
          toast.success(`Saved ${saved.name}`)
          onOpenChange(false)
        },
        onError: (error) =>
          toast.error(
            getApiErrorMessage(error, {
              name_taken: 'Another group already has that name.',
            }),
          ),
      },
    )
  }

  return (
    <Dialog open onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>
            {group ? `Edit ${group.name}` : 'New tester group'}
          </DialogTitle>
          <DialogDescription>
            Send a build to everyone in the group from its build page. Members
            only receive builds of apps they can open.
          </DialogDescription>
        </DialogHeader>
        <Stack gap={4}>
          <Stack gap={2}>
            <Label htmlFor="tester-group-name">Name</Label>
            <Input
              id="tester-group-name"
              value={name}
              maxLength={60}
              placeholder="QA team"
              onChange={(event) => setName(event.target.value)}
            />
          </Stack>
          <Stack gap={2}>
            <Label htmlFor="tester-group-search">
              Members ({members.size})
            </Label>
            <Input
              id="tester-group-search"
              type="search"
              value={search}
              placeholder="Search people"
              onChange={(event) => setSearch(event.target.value)}
            />
            <ul className="max-h-64 space-y-1 overflow-y-auto rounded-md border p-2">
              {users.map((user) => {
                const id = `tester-group-member-${user.id}`
                return (
                  <li key={user.id} className="flex items-center gap-2 py-1">
                    <Checkbox
                      id={id}
                      checked={members.has(user.id)}
                      onCheckedChange={(checked) =>
                        toggle(user.id, user.email, checked)
                      }
                    />
                    <Label
                      htmlFor={id}
                      className="min-w-0 truncate font-normal"
                    >
                      {user.display_name
                        ? `${user.display_name} (${user.email})`
                        : user.email}
                    </Label>
                  </li>
                )
              })}
              {usersQuery.isSuccess && users.length === 0 ? (
                <li>
                  <Text variant="muted">No one matches.</Text>
                </li>
              ) : null}
            </ul>
          </Stack>
        </Stack>
        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button disabled={!name.trim() || save.isPending} onClick={submit}>
            {save.isPending ? 'Saving...' : 'Save'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

function TesterGroupsPage() {
  const groupsQuery = useTesterGroups()
  const groups = groupsQuery.data?.groups ?? []
  const remove = useDeleteTesterGroup()
  // undefined: closed; null: a new group.
  const [editing, setEditing] = useState<TesterGroup | null | undefined>()

  return (
    <PageLayout>
      <PageMeta title="Tester groups" noindex />
      <PageHeader
        title="Tester groups"
        description="People who try builds before they go to a store. Send a build to a whole group from its page."
        actions={
          <Button onClick={() => setEditing(null)}>
            <HugeiconsIcon icon={Add01Icon} />
            New group
          </Button>
        }
      />
      {groupsQuery.isSuccess && groups.length === 0 ? (
        <Empty className="border bg-card">
          <EmptyHeader>
            <EmptyMedia variant="icon">
              <HugeiconsIcon icon={UserGroupIcon} />
            </EmptyMedia>
            <EmptyTitle>No tester groups yet</EmptyTitle>
            <EmptyDescription>
              Group the people who test your apps, then send them a build with
              one click. Each gets an email with their own install link.
            </EmptyDescription>
          </EmptyHeader>
          <EmptyContent>
            <Button onClick={() => setEditing(null)}>New group</Button>
          </EmptyContent>
        </Empty>
      ) : (
        <Stack gap={3}>
          {groups.map((group) => (
            <Card key={group.id} size="sm">
              <CardHeader>
                <Row gap={3} className="justify-between">
                  <CardTitle>{group.name}</CardTitle>
                  <Row gap={2}>
                    <Button
                      size="sm"
                      variant="outline"
                      onClick={() => setEditing(group)}
                    >
                      Edit
                    </Button>
                    <Button
                      size="sm"
                      variant="ghost"
                      disabled={remove.isPending}
                      onClick={() =>
                        remove.mutate(group.id, {
                          onSuccess: () =>
                            toast.success(`Deleted ${group.name}`),
                          onError: (error) =>
                            toast.error(getApiErrorMessage(error, {})),
                        })
                      }
                    >
                      Delete
                    </Button>
                  </Row>
                </Row>
              </CardHeader>
              <CardContent>
                <Text variant="muted">
                  {group.members.length
                    ? group.members.map(memberLabel).join(', ')
                    : 'No members yet.'}
                </Text>
              </CardContent>
            </Card>
          ))}
        </Stack>
      )}
      {editing !== undefined ? (
        <GroupDialog
          group={editing}
          onOpenChange={(open) => {
            if (!open) setEditing(undefined)
          }}
        />
      ) : null}
    </PageLayout>
  )
}
