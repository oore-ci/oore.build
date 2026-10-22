import { createFileRoute } from '@tanstack/react-router'
import {
  type RowSelectionState,
  type SortingState,
} from '@tanstack/react-table'
import { useCallback, useMemo, useState } from 'react'
import { toast } from '@/lib/toast'

import { getColumns } from './-users-columns'
import { UsersToolbar } from './-users-toolbar'
import type { UserRole } from '@oore/client/models'
import {
  resolveDataTableSorting,
  type SortDirection,
} from '@/components/data-table-features'
import ConfirmDialog from '@/components/ConfirmDialog'
import {
  useDeleteUser,
  useReEnableUser,
  useUpdateUserRole,
  useUsers,
} from '@/hooks/use-auth'
import { useAuthStore } from '@/stores/auth-store'
import { searchChoice, searchNumber, searchString } from '@/lib/search-input'
import type { SearchInput } from '@/lib/search-input'
import { usePageClamp } from '@/hooks/use-page-clamp'
import {
  getActiveInstanceOrRedirect,
  requireInstanceRoleOrRedirect,
} from '@/lib/instance-context'
import { isOoreApiError } from '@oore/client/client'
import PageLayout from '@/components/page-layout'
import PageHeader from '@/components/page-header'
import { useDataTable } from '@/components/data-table'
import { PageMeta } from '@/lib/seo'
import { InviteUserAction } from './-invite-user-action'
import { UserCsvActions } from './-user-csv-actions'
import { UsersEmptyState } from './-users-empty-state'
import { UsersCollection } from './-users-collection'

export type UserSort = 'created_at' | 'email' | 'role' | 'status'

interface UsersSearch {
  direction?: SortDirection
  page?: number
  pageSize?: 20 | 50 | 100
  q?: string
  sort?: UserSort
}

const USER_SORTS = ['created_at', 'email', 'role', 'status'] as const

export function parseUsersSearch(search: SearchInput): UsersSearch {
  const page = searchNumber(search, 'page')
  const pageSize = searchNumber(search, 'pageSize')
  const q = searchString(search, 'q')?.trim() ?? ''
  const sort = searchChoice(search, 'sort', USER_SORTS)
  const direction = searchString(search, 'direction')

  return {
    q: q || undefined,
    sort,
    direction:
      direction === 'asc' || direction === 'desc' ? direction : undefined,
    page: Number.isInteger(page) && page > 1 ? page : undefined,
    pageSize: pageSize === 50 || pageSize === 100 ? pageSize : undefined,
  }
}

export const Route = createFileRoute('/settings/users')({
  staticData: {
    breadcrumb: {
      title: 'Users',
    },
  },
  validateSearch: parseUsersSearch,
  beforeLoad: (context) => {
    const instance = getActiveInstanceOrRedirect()
    requireInstanceRoleOrRedirect(instance.id, ['owner', 'admin'], context)
  },
  component: UsersSettingsPage,
})

interface ConfirmAction {
  type: 'disable' | 'role_change' | 'bulk_disable'
  userId: string
  userEmail: string
  newRole?: UserRole
  userIds?: Array<string>
}

function UsersSettingsPage() {
  const authUser = useAuthStore((state) => state.user)
  const updateRoleMutation = useUpdateUserRole()
  const deleteMutation = useDeleteUser()
  const reEnableMutation = useReEnableUser()
  const search = Route.useSearch()
  const navigate = Route.useNavigate()
  const [confirmAction, setConfirmAction] = useState<ConfirmAction | null>(null)
  const [rowSelection, setRowSelection] = useState<RowSelectionState>({})

  const page = search.page ?? 1
  const pageSize = search.pageSize ?? 20
  const sort = search.sort ?? 'created_at'
  const direction = search.direction ?? 'desc'
  const usersQuery = useUsers({
    q: search.q,
    sort,
    direction,
    limit: pageSize,
    offset: (page - 1) * pageSize,
  })
  const users = usersQuery.data?.users ?? []

  const updateSearch = useCallback(
    (updates: Partial<UsersSearch>) => {
      setRowSelection({})
      void navigate({
        search: (previous) => ({ ...previous, ...updates }),
        replace: true,
      })
    },
    [navigate],
  )

  const showError = useCallback((cause: unknown, fallback: string) => {
    toast.error(isOoreApiError(cause) ? cause.message : fallback)
  }, [])

  const handleReEnable = useCallback(
    (userId: string, email: string) => {
      reEnableMutation.mutate(userId, {
        onSuccess: () => toast.success(`${email} has been re-enabled`),
        onError: (error) => showError(error, 'Failed to re-enable user'),
      })
    },
    [reEnableMutation, showError],
  )

  const columns = useMemo(
    () =>
      getColumns({
        authUserId: authUser?.user_id,
        onRoleChange: (userId, email, newRole) =>
          setConfirmAction({
            type: 'role_change',
            userId,
            userEmail: email,
            newRole,
          }),
        onDisable: (userId, email) =>
          setConfirmAction({
            type: 'disable',
            userId,
            userEmail: email,
          }),
        onReEnable: handleReEnable,
      }),
    [authUser?.user_id, handleReEnable],
  )

  const sorting = useMemo<SortingState>(
    () => [{ id: sort, desc: direction === 'desc' }],
    [direction, sort],
  )

  const table = useDataTable({
    data: users,
    columns,
    enableRowSelection: (row) =>
      row.original.role !== 'owner' && row.original.id !== authUser?.user_id,
    state: {
      rowSelection,
      sorting,
    },
    onRowSelectionChange: setRowSelection,
    onSortingChange: (updater) => {
      const next = resolveDataTableSorting(updater, sorting, USER_SORTS)
      if (next) handleSortChange(next.sort, next.direction)
    },
  })

  const filteredTotal = usersQuery.data?.total ?? 0

  usePageClamp(
    page,
    pageSize,
    usersQuery.isLoading ? undefined : filteredTotal,
    (nextPage) => {
      updateSearch({ page: nextPage === 1 ? undefined : nextPage })
    },
  )

  const handleConfirm = async () => {
    if (!confirmAction) return

    if (confirmAction.type === 'bulk_disable' && confirmAction.userIds) {
      const ids = confirmAction.userIds
      const results = await Promise.allSettled(
        ids.map((id) => deleteMutation.mutateAsync(id)),
      )
      const failed = results.filter(
        (result) => result.status === 'rejected',
      ).length
      if (failed === 0) {
        toast.success(`${ids.length} user(s) disabled`)
      } else {
        toast.error(`${failed} of ${ids.length} disable(s) failed`)
      }
      setConfirmAction(null)
      setRowSelection({})
      return
    }

    if (confirmAction.type === 'disable') {
      deleteMutation.mutate(confirmAction.userId, {
        onSuccess: () => {
          toast.success(`${confirmAction.userEmail} has been disabled`)
          setConfirmAction(null)
          setRowSelection({})
        },
        onError: (error) => {
          showError(error, 'Failed to disable user')
          setConfirmAction(null)
        },
      })
      return
    }

    if (confirmAction.newRole) {
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
          onError: (error) => {
            showError(error, 'Failed to update role')
            setConfirmAction(null)
          },
        },
      )
    }
  }

  const confirmTitle = !confirmAction
    ? ''
    : confirmAction.type === 'bulk_disable'
      ? `Disable ${confirmAction.userIds?.length ?? 0} user(s)?`
      : confirmAction.type === 'disable'
        ? `Disable ${confirmAction.userEmail}?`
        : `Change role for ${confirmAction.userEmail}?`
  const confirmDescription = !confirmAction
    ? ''
    : confirmAction.type === 'role_change'
      ? `Change role from current to ${confirmAction.newRole?.replace('_', ' ') ?? ''}?`
      : 'This will revoke all active sessions. You can re-enable the affected users later.'
  function handleSortChange(nextSort: UserSort, next: SortDirection) {
    updateSearch({ sort: nextSort, direction: next, page: undefined })
  }

  return (
    <PageLayout width="wide" fill>
      <PageMeta title="Users" noindex />
      <PageHeader
        title="Users"
        description="Instance access, roles, and account status."
        actions={
          <div className="flex flex-wrap gap-2">
            <UserCsvActions users={users} />
            <InviteUserAction />
          </div>
        }
      />

      <UsersCollection
        toolbar={
          <UsersToolbar
            table={table}
            onBulkDisable={(userIds) =>
              setConfirmAction({
                type: 'bulk_disable',
                userId: '',
                userEmail: '',
                userIds,
              })
            }
          />
        }
        emptyState={
          <UsersEmptyState
            onClearSearch={() =>
              updateSearch({ q: undefined, page: undefined })
            }
            state={search.q ? 'no-results' : 'empty'}
          />
        }
        error={usersQuery.error}
        isLoading={usersQuery.isLoading}
        isRefreshing={usersQuery.isFetching && !usersQuery.isLoading}
        onPageChange={(nextPage) =>
          updateSearch({
            page: nextPage > 1 ? nextPage : undefined,
          })
        }
        onRetry={() => void usersQuery.refetch()}
        onSearch={(value) =>
          updateSearch({ q: value.trim() || undefined, page: undefined })
        }
        page={page}
        pageSize={pageSize}
        query={search.q ?? ''}
        table={table}
        total={filteredTotal}
      />

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
        isPending={deleteMutation.isPending || updateRoleMutation.isPending}
        onConfirm={() => void handleConfirm()}
      />
    </PageLayout>
  )
}
