import { createFileRoute, redirect } from '@tanstack/react-router'
import {
  getCoreRowModel,
  getFilteredRowModel,
  getPaginationRowModel,
  getSortedRowModel,
  useReactTable,
} from '@tanstack/react-table'
import { useCallback, useMemo, useState } from 'react'
import { toast } from 'sonner'
import { getColumns } from './-users-columns'
import { UsersToolbar } from './-users-toolbar'
import type {
  ColumnFiltersState,
  RowSelectionState,
  SortingState,
} from '@tanstack/react-table'

import type { User, UserRole } from '@/lib/types'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { DataTable } from '@/components/ui/data-table'
import { Input } from '@/components/ui/input'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Skeleton } from '@/components/ui/skeleton'
import { Spinner } from '@/components/ui/spinner'
import ConfirmDialog from '@/components/ConfirmDialog'
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
import { ApiClientError } from '@/lib/api'
import PageLayout from '@/components/page-layout'
import PageHeader from '@/components/page-header'
import { PageMeta } from '@/lib/seo'

export const Route = createFileRoute('/settings/users')({
  staticData: { breadcrumbLabel: 'Users' },
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireAuthOrRedirect(instance.id)

    // Check that the current user has admin/owner role
    const user = useAuthStore.getState().user
    if (!user || (user.role !== 'owner' && user.role !== 'admin')) {
      throw redirect({ to: '/' })
    }
  },
  component: UsersSettingsPage,
})

const EMAIL_RE = /^[^\s@]+@[^\s@]+\.[^\s@]+$/
const EMPTY_USERS: Array<User> = []

const ROLE_OPTIONS: Record<string, string> = {
  admin: 'Admin',
  developer: 'Developer',
  qa_viewer: 'QA Viewer',
}

const ROLE_DESCRIPTIONS: Record<string, string> = {
  owner:
    'Full access. Can manage billing, delete the instance, and configure all settings.',
  admin:
    'Can manage users, integrations, and all projects. Cannot delete the instance.',
  developer:
    'Can create and manage projects, pipelines, and builds. Cannot manage users or integrations.',
  qa_viewer:
    'Read-only access to builds and artifacts. Cannot modify projects or settings.',
}

interface ConfirmAction {
  type: 'disable' | 'role_change' | 'bulk_disable'
  userId: string
  userEmail: string
  newRole?: UserRole
  userIds?: Array<string>
}

function useUsersSettingsPageState() {
  const authUser = useAuthStore((s) => s.user)
  const { data, isLoading, error } = useUsers()
  const inviteMutation = useInviteUser()
  const updateRoleMutation = useUpdateUserRole()
  const deleteMutation = useDeleteUser()
  const reEnableMutation = useReEnableUser()

  const [inviteEmail, setInviteEmail] = useState('')
  const [inviteRole, setInviteRole] = useState<UserRole>('developer')
  const [inviteError, setInviteError] = useState<string | null>(null)
  const [emailError, setEmailError] = useState<string | null>(null)
  const [confirmAction, setConfirmAction] = useState<ConfirmAction | null>(null)

  const [sorting, setSorting] = useState<SortingState>([])
  const [columnFilters, setColumnFilters] = useState<ColumnFiltersState>([])
  const [rowSelection, setRowSelection] = useState<RowSelectionState>({})

  const showError = useCallback((err: unknown, fallback: string) => {
    const message = err instanceof ApiClientError ? err.message : fallback
    toast.error(message)
  }, [])

  const handleInvite = () => {
    setInviteError(null)
    inviteMutation.mutate(
      { email: inviteEmail, role: inviteRole },
      {
        onSuccess: () => {
          const instanceUrl = window.location.origin
          void navigator.clipboard.writeText(instanceUrl).then(
            () => {
              toast.success(
                `${inviteEmail} invited — instance URL copied to clipboard`,
                {
                  description: `Share this with them: ${instanceUrl}`,
                  duration: 8000,
                },
              )
            },
            () => {
              toast.success(`${inviteEmail} invited`, {
                description: `Share this URL with them: ${instanceUrl}`,
                duration: 8000,
              })
            },
          )
          setInviteEmail('')
          setInviteRole('developer')
        },
        onError: (e) => {
          setInviteError(
            e instanceof Error ? e.message : 'Failed to invite user',
          )
        },
      },
    )
  }

  const handleConfirm = () => {
    if (!confirmAction) return

    if (confirmAction.type === 'bulk_disable' && confirmAction.userIds) {
      const ids = confirmAction.userIds
      let completed = 0
      let failed = 0
      for (const id of ids) {
        deleteMutation.mutate(id, {
          onSuccess: () => {
            completed++
            if (completed + failed === ids.length) {
              if (failed === 0) {
                toast.success(`${completed} user(s) disabled`)
              } else {
                toast.error(`${failed} of ${ids.length} disable(s) failed`)
              }
              setConfirmAction(null)
              setRowSelection({})
            }
          },
          onError: () => {
            failed++
            if (completed + failed === ids.length) {
              toast.error(`${failed} of ${ids.length} disable(s) failed`)
              setConfirmAction(null)
              setRowSelection({})
            }
          },
        })
      }
    } else if (confirmAction.type === 'disable') {
      deleteMutation.mutate(confirmAction.userId, {
        onSuccess: () => {
          toast.success(`${confirmAction.userEmail} has been disabled`)
          setConfirmAction(null)
          setRowSelection({})
        },
        onError: (err) => {
          showError(err, 'Failed to disable user')
          setConfirmAction(null)
        },
      })
    } else if (confirmAction.newRole) {
      updateRoleMutation.mutate(
        {
          userId: confirmAction.userId,
          data: { role: confirmAction.newRole },
        },
        {
          onSuccess: () => {
            toast.success(`Role updated for ${confirmAction.userEmail}`)
            setConfirmAction(null)
          },
          onError: (err) => {
            showError(err, 'Failed to update role')
            setConfirmAction(null)
          },
        },
      )
    }
  }

  const handleReEnable = useCallback(
    (userId: string, email: string) => {
      reEnableMutation.mutate(userId, {
        onSuccess: () => {
          toast.success(`${email} has been re-enabled`)
        },
        onError: (err) => {
          showError(err, 'Failed to re-enable user')
        },
      })
    },
    [reEnableMutation, showError],
  )

  const users = data?.users ?? EMPTY_USERS
  const userStatusCounts = useMemo(
    () =>
      users.reduce(
        (counts, user) => {
          counts[user.status] += 1
          return counts
        },
        { active: 0, disabled: 0, invited: 0 },
      ),
    [users],
  )

  const columns = useMemo(
    () =>
      getColumns({
        authUserId: authUser?.user_id,
        onRoleChange: (userId, email, newRole) => {
          setConfirmAction({
            type: 'role_change',
            userId,
            userEmail: email,
            newRole,
          })
        },
        onDisable: (userId, email) => {
          setConfirmAction({
            type: 'disable',
            userId,
            userEmail: email,
          })
        },
        onReEnable: handleReEnable,
      }),
    [authUser?.user_id, handleReEnable],
  )

  const table = useReactTable({
    data: users,
    columns,
    getCoreRowModel: getCoreRowModel(),
    getSortedRowModel: getSortedRowModel(),
    getFilteredRowModel: getFilteredRowModel(),
    getPaginationRowModel: getPaginationRowModel(),
    enableRowSelection: (row) =>
      row.original.role !== 'owner' && row.original.id !== authUser?.user_id,
    state: { sorting, columnFilters, rowSelection },
    onSortingChange: setSorting,
    onColumnFiltersChange: setColumnFilters,
    onRowSelectionChange: setRowSelection,
    initialState: { pagination: { pageSize: 20 } },
  })

  const pendingMutation =
    deleteMutation.isPending || updateRoleMutation.isPending

  const handleBulkDisable = (userIds: Array<string>) => {
    setConfirmAction({
      type: 'bulk_disable',
      userId: '',
      userEmail: '',
      userIds,
    })
  }

  const confirmTitle = (() => {
    if (!confirmAction) return ''
    if (confirmAction.type === 'bulk_disable') {
      return `Disable ${confirmAction.userIds?.length ?? 0} user(s)?`
    }
    if (confirmAction.type === 'disable') {
      return `Disable ${confirmAction.userEmail}?`
    }
    return `Change role for ${confirmAction.userEmail}?`
  })()

  const confirmDescription = (() => {
    if (!confirmAction) return ''
    if (confirmAction.type === 'bulk_disable') {
      return 'This will revoke all their active sessions. You can re-enable them later.'
    }
    if (confirmAction.type === 'disable') {
      return 'This will revoke all their active sessions. You can re-enable them later.'
    }
    return `Change role from current to ${confirmAction.newRole?.replace('_', ' ') ?? ''}?`
  })()

  if (isLoading) {
    return { status: 'loading' as const }
  }

  if (error) {
    return {
      status: 'error' as const,
      message: error instanceof Error ? error.message : 'Unknown error',
    }
  }

  return {
    status: 'ready' as const,
    confirmAction,
    confirmDescription,
    confirmTitle,
    emailError,
    handleBulkDisable,
    handleConfirm,
    handleInvite,
    inviteEmail,
    inviteError,
    inviteMutation,
    inviteRole,
    pendingMutation,
    setConfirmAction,
    setEmailError,
    setInviteEmail,
    setInviteRole,
    table,
    users,
    userStatusCounts,
  }
}

function UsersSettingsPage() {
  const pageState = useUsersSettingsPageState()

  if (pageState.status === 'loading') {
    return (
      <PageLayout width="wide">
        <PageMeta title="User Management" noindex />
        <div className="space-y-2">
          <Skeleton className="h-8 w-48" />
          <Skeleton className="h-4 w-80" />
        </div>
        <Skeleton className="h-32 w-full" />
        <div className="space-y-2">
          <Skeleton className="h-10 w-full" />
          <Skeleton className="h-10 w-full" />
          <Skeleton className="h-10 w-full" />
        </div>
      </PageLayout>
    )
  }

  if (pageState.status === 'error') {
    return (
      <PageLayout>
        <PageMeta title="User Management" noindex />
        <Alert variant="destructive">
          <AlertDescription>
            Failed to load users: {pageState.message}
          </AlertDescription>
        </Alert>
      </PageLayout>
    )
  }

  const {
    confirmAction,
    confirmDescription,
    confirmTitle,
    emailError,
    handleBulkDisable,
    handleConfirm,
    handleInvite,
    inviteEmail,
    inviteError,
    inviteMutation,
    inviteRole,
    pendingMutation,
    setConfirmAction,
    setEmailError,
    setInviteEmail,
    setInviteRole,
    table,
    users,
    userStatusCounts,
  } = pageState

  return (
    <PageLayout width="wide">
      <PageMeta title="User Management" noindex />
      <PageHeader
        title="Users"
        description="Manage team members and their roles."
      />
      <section className="grid gap-4 md:grid-cols-3">
        <Card>
          <CardContent>
            <p className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
              Total users
            </p>
            <p className="mt-3 text-2xl font-bold tracking-tight">
              {users.length}
            </p>
            <p className="mt-1 text-xs text-muted-foreground">
              Active + invited + disabled
            </p>
          </CardContent>
        </Card>
        <Card>
          <CardContent>
            <p className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
              Active users
            </p>
            <p className="mt-3 text-2xl font-bold tracking-tight">
              {userStatusCounts.active}
            </p>
            <p className="mt-1 text-xs text-muted-foreground">
              Can access this instance
            </p>
          </CardContent>
        </Card>
        <Card>
          <CardContent>
            <p className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
              Invited users
            </p>
            <p className="mt-3 text-2xl font-bold tracking-tight">
              {userStatusCounts.invited}
            </p>
            <p className="mt-1 text-xs text-muted-foreground">
              Pending account completion
            </p>
          </CardContent>
        </Card>
      </section>
      {/* Invite form */}
      <Card>
        <CardHeader>
          <CardTitle className="text-sm font-medium uppercase tracking-wider text-muted-foreground">
            Invite user
          </CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="flex flex-col gap-3 sm:flex-row">
            <div className="flex flex-1 flex-col gap-1">
              <Input
                type="email"
                value={inviteEmail}
                onChange={(e) => {
                  setInviteEmail(e.target.value)
                  if (emailError) setEmailError(null)
                }}
                onBlur={() => {
                  if (
                    inviteEmail.trim() &&
                    !EMAIL_RE.test(inviteEmail.trim())
                  ) {
                    setEmailError('Please enter a valid email address')
                  }
                }}
                placeholder="email@example.com"
              />
              {emailError ? (
                <p className="text-xs text-destructive">{emailError}</p>
              ) : null}
            </div>
            <Select
              value={inviteRole}
              onValueChange={(v) => setInviteRole(v as UserRole)}
              items={ROLE_OPTIONS}
            >
              <SelectTrigger className="w-full sm:w-36">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {Object.entries(ROLE_OPTIONS).map(([key, value]) => (
                  <SelectItem key={key} value={key}>
                    {value}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            <Button
              className="w-full sm:w-auto"
              onClick={handleInvite}
              disabled={
                !inviteEmail || !!emailError || inviteMutation.isPending
              }
            >
              {inviteMutation.isPending ? (
                <>
                  <Spinner className="size-4" />
                  Inviting...
                </>
              ) : (
                'Invite'
              )}
            </Button>
          </div>
          <p className="text-xs text-muted-foreground">
            {ROLE_DESCRIPTIONS[inviteRole] ?? ''}
          </p>
          {inviteError ? (
            <p className="text-sm text-destructive">{inviteError}</p>
          ) : null}
        </CardContent>
      </Card>

      {/* Users data table */}
      <Card>
        <CardHeader>
          <CardTitle className="text-sm font-medium uppercase tracking-wider text-muted-foreground">
            Team Access Inventory
          </CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          <UsersToolbar table={table} onBulkDisable={handleBulkDisable} />
          <DataTable table={table} />
          {table.getPageCount() > 1 ? (
            <div className="flex items-center justify-between pt-2">
              <span className="text-xs text-muted-foreground">
                {table.getFilteredRowModel().rows.length} user(s)
              </span>
              <div className="flex items-center gap-2">
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => table.previousPage()}
                  disabled={!table.getCanPreviousPage()}
                >
                  Previous
                </Button>
                <span className="text-xs text-muted-foreground">
                  Page {table.getState().pagination.pageIndex + 1} of{' '}
                  {table.getPageCount()}
                </span>
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => table.nextPage()}
                  disabled={!table.getCanNextPage()}
                >
                  Next
                </Button>
              </div>
            </div>
          ) : null}
        </CardContent>
      </Card>

      {/* Confirmation dialog */}
      <ConfirmDialog
        open={confirmAction !== null}
        onOpenChange={(open) => {
          if (!open) setConfirmAction(null)
        }}
        title={confirmTitle}
        description={confirmDescription}
        confirmLabel={
          confirmAction?.type === 'role_change' ? 'Change role' : 'Disable'
        }
        confirmVariant={
          confirmAction?.type === 'role_change' ? 'default' : 'destructive'
        }
        isPending={pendingMutation}
        onConfirm={handleConfirm}
      />
    </PageLayout>
  )
}
