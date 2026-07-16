import { createFileRoute, redirect } from '@tanstack/react-router'
import {
  flexRender,
  getCoreRowModel,
  getFilteredRowModel,
  getPaginationRowModel,
  getSortedRowModel,
  useReactTable,
} from '@tanstack/react-table'
import type {
  Row,
  RowSelectionState,
  SortingState,
} from '@tanstack/react-table'
import { useCallback, useMemo, useState } from 'react'
import { toast } from '@/lib/toast'

import { getColumns } from './-users-columns'
import { UsersToolbar } from './-users-toolbar'
import type { User, UserRole } from '@/lib/types'
import type { SortDirection } from '@/components/collection-controls'
import {
  CollectionPagination,
  SortableTableHead,
} from '@/components/collection-controls'
import { Checkbox } from '@/components/ui/checkbox'
import { Skeleton } from '@/components/ui/skeleton'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import ConfirmDialog from '@/components/ConfirmDialog'
import {
  useDeleteUser,
  useReEnableUser,
  useUpdateUserRole,
  useUsers,
} from '@/hooks/use-auth'
import { useAuthStore } from '@/stores/auth-store'
import { usePageClamp } from '@/hooks/use-page-clamp'
import {
  getActiveInstanceOrRedirect,
  requireAuthOrRedirect,
} from '@/lib/instance-context'
import { ApiClientError } from '@/lib/api'
import PageLayout from '@/components/page-layout'
import PageHeader from '@/components/page-header'
import { PageMeta } from '@/lib/seo'
import { InviteUserAction } from './-invite-user-action'
import { UsersSummary } from './-users-summary'
import { UsersEmptyState } from './-users-empty-state'
import { UsersErrorAlert } from './-users-error-alert'

export type UserSort = 'created_at' | 'email' | 'role' | 'status'

interface UsersSearch {
  direction?: SortDirection
  page?: number
  pageSize?: 20 | 50 | 100
  q?: string
  sort?: UserSort
}

const USER_SORTS = new Set<UserSort>(['created_at', 'email', 'role', 'status'])

export function parseUsersSearch(search: Record<string, unknown>): UsersSearch {
  const page = Number(search.page)
  const pageSize = Number(search.pageSize)
  const q = typeof search.q === 'string' ? search.q.trim() : ''
  const sort = search.sort as UserSort

  return {
    q: q || undefined,
    sort: USER_SORTS.has(sort) ? sort : undefined,
    direction:
      search.direction === 'asc' || search.direction === 'desc'
        ? search.direction
        : undefined,
    page: Number.isInteger(page) && page > 1 ? page : undefined,
    pageSize: pageSize === 50 || pageSize === 100 ? pageSize : undefined,
  }
}

export const Route = createFileRoute('/settings/users')({
  staticData: { breadcrumbLabel: 'Users' },
  validateSearch: parseUsersSearch,
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

const EMPTY_USERS: Array<User> = []
interface ConfirmAction {
  type: 'disable' | 'role_change' | 'bulk_disable'
  userId: string
  userEmail: string
  newRole?: UserRole
  userIds?: Array<string>
}

function useUsersSettingsPageState() {
  const authUser = useAuthStore((state) => state.user)
  const usersQuery = useUsers()
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
  const users = usersQuery.data?.users ?? EMPTY_USERS

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

  const showError = useCallback((error: unknown, fallback: string) => {
    toast.error(error instanceof ApiClientError ? error.message : fallback)
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

  const table = useReactTable({
    data: users,
    columns,
    getCoreRowModel: getCoreRowModel(),
    getSortedRowModel: getSortedRowModel(),
    getFilteredRowModel: getFilteredRowModel(),
    getPaginationRowModel: getPaginationRowModel(),
    globalFilterFn: (row, _columnId, filterValue) => {
      const query = String(filterValue).trim().toLocaleLowerCase()
      if (!query) return true
      const user = row.original
      return [user.email, user.display_name, user.role, user.status].some(
        (value) => value?.toLocaleLowerCase().includes(query),
      )
    },
    enableRowSelection: (row) =>
      row.original.role !== 'owner' && row.original.id !== authUser?.user_id,
    state: {
      globalFilter: search.q ?? '',
      pagination: { pageIndex: page - 1, pageSize },
      rowSelection,
      sorting,
    },
    onRowSelectionChange: setRowSelection,
  })

  const filteredTotal = table.getFilteredRowModel().rows.length

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

  return {
    confirmAction,
    confirmDescription,
    confirmTitle,
    direction,
    filteredTotal,
    handleConfirm,
    page,
    pageSize,
    search,
    setConfirmAction,
    sort,
    table,
    updateSearch,
    userStatusCounts,
    users,
    usersQuery,
    pendingMutation: deleteMutation.isPending || updateRoleMutation.isPending,
  }
}

function renderUserCell(row: Row<User>, columnId: string) {
  const cell = row
    .getVisibleCells()
    .find((candidate) => candidate.column.id === columnId)
  return cell ? flexRender(cell.column.columnDef.cell, cell.getContext()) : null
}

function UsersSettingsPage() {
  const pageState = useUsersSettingsPageState()
  const {
    confirmAction,
    confirmDescription,
    confirmTitle,
    direction,
    filteredTotal,
    handleConfirm,
    page,
    pageSize,
    search,
    setConfirmAction,
    sort,
    table,
    updateSearch,
    userStatusCounts,
    users,
    usersQuery,
  } = pageState
  const rows = table.getRowModel().rows
  const showTrueEmpty =
    !usersQuery.isLoading && !usersQuery.error && users.length === 0
  const showFilteredEmpty =
    !usersQuery.isLoading &&
    !usersQuery.error &&
    users.length > 0 &&
    filteredTotal === 0

  function handleSortChange(nextSort: UserSort, next: SortDirection) {
    updateSearch({ sort: nextSort, direction: next, page: undefined })
  }

  return (
    <PageLayout width="wide">
      <PageMeta title="User Management" noindex />
      <PageHeader
        title="Users"
        description="Manage instance roles and assign project access from each project’s Settings tab."
        actions={<InviteUserAction />}
      />

      {!usersQuery.error ? (
        <UsersSummary
          counts={{
            active: userStatusCounts.active,
            invited: userStatusCounts.invited,
            total: users.length,
          }}
          isLoading={usersQuery.isLoading}
        />
      ) : null}

      {usersQuery.error ? (
        <UsersErrorAlert
          error={usersQuery.error}
          onRetry={() => void usersQuery.refetch()}
        />
      ) : (
        <section aria-label="User inventory" className="min-w-0 space-y-4">
          <UsersToolbar
            key={search.q ?? ''}
            table={table}
            initialSearch={search.q ?? ''}
            sort={sort}
            direction={direction}
            onSearch={(value) =>
              updateSearch({ q: value.trim() || undefined, page: undefined })
            }
            onSortChange={handleSortChange}
            onBulkDisable={(userIds) =>
              setConfirmAction({
                type: 'bulk_disable',
                userId: '',
                userEmail: '',
                userIds,
              })
            }
          />

          <UsersEmptyState
            onClearSearch={() =>
              updateSearch({ q: undefined, page: undefined })
            }
            state={
              showTrueEmpty ? 'empty' : showFilteredEmpty ? 'no-results' : null
            }
          />

          {usersQuery.isLoading || filteredTotal > 0 ? (
            <>
              <div className="divide-y sm:hidden">
                {usersQuery.isLoading
                  ? Array.from({ length: 5 }, (_, index) => (
                      <div key={index} className="flex items-start gap-3 py-4">
                        <Skeleton className="mt-1 size-4" />
                        <div className="flex-1 space-y-2">
                          <Skeleton className="h-4 w-2/3" />
                          <Skeleton className="h-5 w-36" />
                        </div>
                        <Skeleton className="size-9" />
                      </div>
                    ))
                  : rows.map((row) => (
                      <div key={row.id} className="flex items-start gap-3 py-4">
                        <Checkbox
                          className="mt-1"
                          checked={row.getIsSelected()}
                          disabled={!row.getCanSelect()}
                          onCheckedChange={(checked) =>
                            row.toggleSelected(!!checked)
                          }
                          aria-label={`Select ${row.original.email}`}
                        />
                        <div className="min-w-0 flex-1 space-y-2">
                          <p className="truncate font-medium">
                            {row.original.email}
                            {row.original.id ===
                            useAuthStore.getState().user?.user_id ? (
                              <span className="ml-2 text-xs text-muted-foreground">
                                (you)
                              </span>
                            ) : null}
                          </p>
                          <div className="flex flex-wrap items-center gap-2">
                            {renderUserCell(row, 'role')}
                            {renderUserCell(row, 'status')}
                          </div>
                        </div>
                        <div className="shrink-0">
                          {renderUserCell(row, 'actions')}
                        </div>
                      </div>
                    ))}
              </div>

              <div className="hidden sm:block">
                <Table>
                  <TableHeader>
                    <TableRow>
                      <TableHead className="w-10">
                        <Checkbox
                          checked={table.getIsAllPageRowsSelected()}
                          indeterminate={
                            table.getIsSomePageRowsSelected() &&
                            !table.getIsAllPageRowsSelected()
                          }
                          onCheckedChange={(checked) =>
                            table.toggleAllPageRowsSelected(!!checked)
                          }
                          aria-label="Select all users on this page"
                        />
                      </TableHead>
                      <SortableTableHead
                        sort={sort}
                        sortKey="email"
                        direction={sort === 'email' ? direction : 'asc'}
                        onSortChange={handleSortChange}
                      >
                        Email
                      </SortableTableHead>
                      <SortableTableHead
                        sort={sort}
                        sortKey="role"
                        direction={direction}
                        onSortChange={handleSortChange}
                      >
                        Role
                      </SortableTableHead>
                      <SortableTableHead
                        sort={sort}
                        sortKey="status"
                        direction={direction}
                        onSortChange={handleSortChange}
                      >
                        Status
                      </SortableTableHead>
                      <SortableTableHead
                        className="hidden lg:table-cell"
                        sort={sort}
                        sortKey="created_at"
                        direction={direction}
                        onSortChange={handleSortChange}
                      >
                        Joined
                      </SortableTableHead>
                      <TableHead className="w-12">
                        <span className="sr-only">Actions</span>
                      </TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {usersQuery.isLoading
                      ? Array.from({ length: 5 }, (_, index) => (
                          <TableRow key={index}>
                            <TableCell>
                              <Skeleton className="size-4" />
                            </TableCell>
                            <TableCell>
                              <Skeleton className="h-4 w-48" />
                            </TableCell>
                            <TableCell>
                              <Skeleton className="h-5 w-20" />
                            </TableCell>
                            <TableCell>
                              <Skeleton className="h-5 w-16" />
                            </TableCell>
                            <TableCell className="hidden lg:table-cell">
                              <Skeleton className="h-4 w-16" />
                            </TableCell>
                            <TableCell>
                              <Skeleton className="size-8" />
                            </TableCell>
                          </TableRow>
                        ))
                      : rows.map((row) => (
                          <TableRow
                            key={row.id}
                            data-state={
                              row.getIsSelected() ? 'selected' : undefined
                            }
                          >
                            {row.getVisibleCells().map((cell) => (
                              <TableCell
                                key={cell.id}
                                className={
                                  cell.column.id === 'created_at'
                                    ? 'hidden lg:table-cell'
                                    : cell.column.id === 'actions'
                                      ? 'text-right'
                                      : undefined
                                }
                              >
                                {flexRender(
                                  cell.column.columnDef.cell,
                                  cell.getContext(),
                                )}
                              </TableCell>
                            ))}
                          </TableRow>
                        ))}
                  </TableBody>
                </Table>
              </div>

              {!usersQuery.isLoading ? (
                <CollectionPagination
                  page={page}
                  pageSize={pageSize}
                  total={filteredTotal}
                  onPageChange={(nextPage) =>
                    updateSearch({
                      page: nextPage > 1 ? nextPage : undefined,
                    })
                  }
                  onPageSizeChange={(nextPageSize) =>
                    updateSearch({
                      page: undefined,
                      pageSize:
                        nextPageSize === 20
                          ? undefined
                          : (nextPageSize as 50 | 100),
                    })
                  }
                />
              ) : null}
            </>
          ) : null}
        </section>
      )}

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
        isPending={pageState.pendingMutation}
        onConfirm={() => void handleConfirm()}
      />
    </PageLayout>
  )
}
