import { For, Show, createMemo, createSignal } from 'solid-js'
import { createFileRoute, redirect } from '@tanstack/solid-router'

import type { UserRole } from '@/lib/types'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { FormError, FormField } from '@/components/ui/form'
import { Input } from '@/components/ui/input'
import { PageHeader } from '@/components/page-header'
import { PageLayout } from '@/components/page-layout'
import { Skeleton } from '@/components/ui/skeleton'
import { Spinner } from '@/components/ui/spinner'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import {
  useDeleteUser,
  useInviteUser,
  useReEnableUser,
  useUpdateUserRole,
  useUsers,
} from '@/hooks/use-auth'
import { useAuthStore } from '@/stores/auth-store'
import {
  getActiveInstanceOrRedirect,
  requireAuthOrRedirect,
} from '@/lib/instance-context'
import { PageMeta } from '@/lib/seo'
import { toast } from '@/components/ui/sonner'

export const Route = createFileRoute('/settings/users')({
  staticData: { breadcrumbLabel: 'Users' },
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireAuthOrRedirect(instance.id)

    const user = useAuthStore.getState().user
    if (!user || (user.role !== 'owner' && user.role !== 'admin')) {
      throw redirect({ to: '/' })
    }
  },
  component: UsersSettingsPage,
})

const ROLE_OPTIONS: Array<{ value: UserRole; label: string }> = [
  { value: 'admin', label: 'Admin' },
  { value: 'developer', label: 'Developer' },
  { value: 'qa_viewer', label: 'QA Viewer' },
]

function statusVariant(status: string) {
  if (status === 'active') return 'success' as const
  if (status === 'invited') return 'info' as const
  return 'secondary' as const
}

function UsersSettingsPage() {
  const authUser = useAuthStore((state) => state.user)
  const usersQuery = useUsers()
  const inviteMutation = useInviteUser()
  const updateRoleMutation = useUpdateUserRole()
  const deleteMutation = useDeleteUser()
  const reEnableMutation = useReEnableUser()

  const [inviteEmail, setInviteEmail] = createSignal('')
  const [inviteRole, setInviteRole] = createSignal<UserRole>('developer')
  const [inviteError, setInviteError] = createSignal<string | null>(null)
  const [filterValue, setFilterValue] = createSignal('')

  const users = () => usersQuery.data?.users ?? []

  const filteredUsers = createMemo(() => {
    const needle = filterValue().trim().toLowerCase()
    if (!needle) return users()
    return users().filter((user) => user.email.toLowerCase().includes(needle))
  })

  const activeCount = createMemo(
    () => users().filter((user) => user.status === 'active').length,
  )
  const invitedCount = createMemo(
    () => users().filter((user) => user.status === 'invited').length,
  )

  const handleInvite = () => {
    const email = inviteEmail().trim()
    if (!email) {
      setInviteError('Email is required.')
      return
    }

    setInviteError(null)
    inviteMutation.mutate(
      { email, role: inviteRole() },
      {
        onSuccess: () => {
          toast.success(`${email} invited`)
          setInviteEmail('')
          setInviteRole('developer')
        },
        onError: (error) => {
          setInviteError(
            error instanceof Error ? error.message : 'Failed to invite user',
          )
        },
      },
    )
  }

  const handleRoleChange = (userId: string, email: string, role: UserRole) => {
    if (!window.confirm(`Change role for ${email} to ${role}?`)) return

    updateRoleMutation.mutate(
      { userId, data: { role } },
      {
        onSuccess: () => {
          toast.success(`Role updated for ${email}`)
        },
        onError: (error) => {
          toast.error(
            error instanceof Error ? error.message : 'Failed to update role',
          )
        },
      },
    )
  }

  const handleDisable = (userId: string, email: string) => {
    if (!window.confirm(`Disable ${email}? This revokes active sessions.`)) return

    deleteMutation.mutate(userId, {
      onSuccess: () => toast.success(`${email} has been disabled`),
      onError: (error) =>
        toast.error(
          error instanceof Error ? error.message : 'Failed to disable user',
        ),
    })
  }

  const handleReEnable = (userId: string, email: string) => {
    reEnableMutation.mutate(userId, {
      onSuccess: () => toast.success(`${email} has been re-enabled`),
      onError: (error) =>
        toast.error(
          error instanceof Error ? error.message : 'Failed to re-enable user',
        ),
    })
  }

  return (
    <PageLayout width="wide" class="space-y-4">
      <PageMeta title="User Management" noindex />
      <PageHeader
        title="Users"
        description="Manage team members and their roles."
      />

      <section class="grid gap-4 md:grid-cols-3">
        <Card>
          <CardContent>
            <p class="text-xs font-medium uppercase tracking-wider text-muted-foreground">
              Total users
            </p>
            <p class="mt-3 text-2xl font-bold tracking-tight">{users().length}</p>
            <p class="mt-1 text-xs text-muted-foreground">Active + invited + disabled</p>
          </CardContent>
        </Card>

        <Card>
          <CardContent>
            <p class="text-xs font-medium uppercase tracking-wider text-muted-foreground">
              Active users
            </p>
            <p class="mt-3 text-2xl font-bold tracking-tight">{activeCount()}</p>
            <p class="mt-1 text-xs text-muted-foreground">Can access this instance</p>
          </CardContent>
        </Card>

        <Card>
          <CardContent>
            <p class="text-xs font-medium uppercase tracking-wider text-muted-foreground">
              Invited users
            </p>
            <p class="mt-3 text-2xl font-bold tracking-tight">{invitedCount()}</p>
            <p class="mt-1 text-xs text-muted-foreground">Pending account completion</p>
          </CardContent>
        </Card>
      </section>

      <Card>
        <CardHeader>
          <CardTitle class="text-sm font-medium uppercase tracking-wider text-muted-foreground">
            Invite user
          </CardTitle>
        </CardHeader>
        <CardContent class="space-y-4">
          <div class="flex gap-3">
            <Input
              type="email"
              value={inviteEmail()}
              onInput={(event) => setInviteEmail(event.currentTarget.value)}
              placeholder="email@example.com"
              class="flex-1"
            />
            <select
              class="h-9 w-36 border border-input bg-background px-2 text-sm"
              value={inviteRole()}
              onChange={(event) =>
                setInviteRole(event.currentTarget.value as UserRole)
              }
            >
              <For each={ROLE_OPTIONS}>
                {(role) => <option value={role.value}>{role.label}</option>}
              </For>
            </select>
            <Button
              onClick={handleInvite}
              disabled={!inviteEmail().trim() || inviteMutation.isPending}
            >
              {inviteMutation.isPending ? (
                <>
                  <Spinner class="size-4" />
                  Inviting...
                </>
              ) : (
                'Invite'
              )}
            </Button>
          </div>

          {inviteError() ? <FormError>{inviteError() ?? ''}</FormError> : null}
        </CardContent>
      </Card>

      <Show when={usersQuery.isLoading}>
        <Card>
          <CardContent class="space-y-2">
            <Skeleton class="h-10 w-full" />
            <Skeleton class="h-10 w-full" />
            <Skeleton class="h-10 w-full" />
          </CardContent>
        </Card>
      </Show>

      <Show when={usersQuery.error}>
        <Alert variant="destructive">
          <AlertDescription>
            Failed to load users: {usersQuery.error?.message}
          </AlertDescription>
        </Alert>
      </Show>

      <Show when={!usersQuery.isLoading && !usersQuery.error}>
        <Card>
          <CardHeader>
            <CardTitle class="text-sm font-medium uppercase tracking-wider text-muted-foreground">
              Team Access Inventory
            </CardTitle>
          </CardHeader>
          <CardContent class="space-y-3">
            <FormField>
              <Input
                value={filterValue()}
                onInput={(event) => setFilterValue(event.currentTarget.value)}
                placeholder="Filter by email..."
                class="max-w-sm"
              />
            </FormField>

            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead class="w-8" />
                  <TableHead>Email</TableHead>
                  <TableHead>Role</TableHead>
                  <TableHead>Status</TableHead>
                  <TableHead class="w-20" />
                </TableRow>
              </TableHeader>
              <TableBody>
                <For each={filteredUsers()}>
                  {(user) => {
                    const selfUser = authUser()?.user_id === user.id
                    const canDisable =
                      user.role !== 'owner' && !selfUser && user.status !== 'disabled'
                    const canRoleChange = user.role !== 'owner' && !selfUser

                    return (
                      <TableRow>
                        <TableCell>
                          <input type="checkbox" disabled class="size-4" />
                        </TableCell>
                        <TableCell>
                          <div class="space-y-1">
                            <p class="font-medium">
                              {user.email}
                              {selfUser ? ' (you)' : ''}
                            </p>
                            <p class="font-mono text-xs text-muted-foreground">
                              {user.id.slice(0, 8)}
                            </p>
                          </div>
                        </TableCell>
                        <TableCell>
                          <Show
                            when={canRoleChange}
                            fallback={
                              <Badge variant="outline">
                                {user.role.replace('_', ' ')}
                              </Badge>
                            }
                          >
                            <select
                              class="h-8 border border-input bg-background px-2 text-sm"
                              value={user.role}
                              onChange={(event) =>
                                handleRoleChange(
                                  user.id,
                                  user.email,
                                  event.currentTarget.value as UserRole,
                                )
                              }
                              disabled={updateRoleMutation.isPending}
                            >
                              <For each={ROLE_OPTIONS}>
                                {(role) => (
                                  <option value={role.value}>{role.label}</option>
                                )}
                              </For>
                            </select>
                          </Show>
                        </TableCell>
                        <TableCell>
                          <Badge variant={statusVariant(user.status)}>
                            {user.status}
                          </Badge>
                        </TableCell>
                        <TableCell class="text-right">
                          <Show
                            when={user.status === 'disabled'}
                            fallback={
                              <Button
                                size="sm"
                                variant="destructive"
                                onClick={() => handleDisable(user.id, user.email)}
                                disabled={!canDisable || deleteMutation.isPending}
                              >
                                Disable
                              </Button>
                            }
                          >
                            <Button
                              size="sm"
                              variant="outline"
                              onClick={() => handleReEnable(user.id, user.email)}
                              disabled={reEnableMutation.isPending}
                            >
                              Re-enable
                            </Button>
                          </Show>
                        </TableCell>
                      </TableRow>
                    )
                  }}
                </For>
              </TableBody>
            </Table>
          </CardContent>
        </Card>
      </Show>
    </PageLayout>
  )
}
