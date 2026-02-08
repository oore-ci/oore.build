import { createFileRoute, redirect, useNavigate } from '@tanstack/react-router'
import {
  getCoreRowModel,
  getFilteredRowModel,
  getSortedRowModel,
  useReactTable,
} from '@tanstack/react-table'
import { useCallback, useEffect, useMemo, useState } from 'react'
import { toast } from 'sonner'
import { getColumns } from './-users-columns'
import { UsersToolbar } from './-users-toolbar'
import type {
  ColumnFiltersState,
  RowSelectionState,
  SortingState,
} from '@tanstack/react-table'

import type { UserRole } from '@/lib/types'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { Card, CardContent } from '@/components/ui/card'
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
import { webPageTitle } from '@/lib/seo'

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

const ROLE_OPTIONS: Record<string, string> = {
  admin: 'Admin',
  developer: 'Developer',
  qa_viewer: 'QA Viewer',
}

interface ConfirmAction {
  type: 'disable' | 'role_change' | 'bulk_disable'
  userId: string
  userEmail: string
  newRole?: UserRole
  userIds?: Array<string>
}

function UsersSettingsPage() {
  const navigate = useNavigate()
  const authUser = useAuthStore((s) => s.user)
  const { data, isLoading, error } = useUsers()
  const inviteMutation = useInviteUser()
  const updateRoleMutation = useUpdateUserRole()
  const deleteMutation = useDeleteUser()
  const reEnableMutation = useReEnableUser()

  const [inviteEmail, setInviteEmail] = useState('')
  const [inviteRole, setInviteRole] = useState<UserRole>('developer')
  const [inviteError, setInviteError] = useState<string | null>(null)
  const [confirmAction, setConfirmAction] = useState<ConfirmAction | null>(null)

  const [sorting, setSorting] = useState<SortingState>([])
  const [columnFilters, setColumnFilters] = useState<ColumnFiltersState>([])
  const [rowSelection, setRowSelection] = useState<RowSelectionState>({})

  useEffect(() => {
    document.title = webPageTitle('User Management')
  }, [])

  // Redirect non-admin users
  useEffect(() => {
    if (authUser && authUser.role !== 'owner' && authUser.role !== 'admin') {
      void navigate({ to: '/' })
    }
  }, [authUser, navigate])

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
          toast.success(`${inviteEmail} invited`)
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

  const users = data?.users ?? []

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
    enableRowSelection: (row) =>
      row.original.role !== 'owner' && row.original.id !== authUser?.user_id,
    state: { sorting, columnFilters, rowSelection },
    onSortingChange: setSorting,
    onColumnFiltersChange: setColumnFilters,
    onRowSelectionChange: setRowSelection,
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
    return (
      <PageLayout>
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

  if (error) {
    return (
      <div className="flex-1 flex items-center justify-center p-6">
        <div className="max-w-md w-full">
          <Alert variant="destructive">
            <AlertDescription>
              Failed to load users:{' '}
              {error instanceof Error ? error.message : 'Unknown error'}
            </AlertDescription>
          </Alert>
        </div>
      </div>
    )
  }

  return (
    <PageLayout>
      <PageHeader
        title="Users"
        description="Manage team members and their roles."
      />
      {/* Invite form */}
      <Card>
        <CardContent className="space-y-4">
          <h2 className="text-sm font-medium">Invite User</h2>
          <div className="flex gap-3">
            <Input
              type="email"
              value={inviteEmail}
              onChange={(e) => setInviteEmail(e.target.value)}
              placeholder="email@example.com"
              className="flex-1"
            />
            <Select
              value={inviteRole}
              onValueChange={(v) => setInviteRole(v as UserRole)}
            >
              <SelectTrigger className="w-36">
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
              onClick={handleInvite}
              disabled={!inviteEmail || inviteMutation.isPending}
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
          {inviteError ? (
            <p className="text-sm text-destructive">{inviteError}</p>
          ) : null}
        </CardContent>
      </Card>

      {/* Users data table */}
      <div className="space-y-4">
        <UsersToolbar table={table} onBulkDisable={handleBulkDisable} />
        <DataTable table={table} />
      </div>

      {/* Confirmation dialog */}
      <ConfirmDialog
        open={confirmAction !== null}
        onOpenChange={(open) => {
          if (!open) setConfirmAction(null)
        }}
        title={confirmTitle}
        description={confirmDescription}
        confirmLabel={
          confirmAction?.type === 'role_change' ? 'Change Role' : 'Disable'
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
