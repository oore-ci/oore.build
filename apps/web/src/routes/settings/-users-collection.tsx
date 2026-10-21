import type { ReactNode } from 'react'

import { CollectionError, CollectionFrame } from '@/components/collection'
import { DataTable, type DataTableInstance } from '@/components/data-table'
import type { User } from '@oore/client/models'

export function UsersCollection({
  emptyState,
  error,
  isLoading,
  isRefreshing,
  onPageChange,
  onRetry,
  onSearch,
  page,
  pageSize,
  query,
  table,
  toolbar,
  total,
}: {
  emptyState: ReactNode
  error: Error | null
  isLoading: boolean
  isRefreshing: boolean
  onPageChange: (page: number) => void
  onRetry: () => void
  onSearch: (query: string) => void
  page: number
  pageSize: number
  query: string
  table: DataTableInstance<User>
  toolbar?: ReactNode
  total: number
}) {
  return (
    <CollectionFrame ariaLabel="Users" isBusy={isLoading || isRefreshing}>
      {error ? (
        <CollectionError
          title="Users could not be loaded"
          description={error.message}
          onRetry={onRetry}
        />
      ) : null}

      {!isLoading && total === 0 && !error ? (
        emptyState
      ) : (
        <DataTable
          table={table}
          filters={toolbar}
          search={{
            value: query,
            onChange: onSearch,
            placeholder: 'Search users',
          }}
          pagination={{ onPageChange, page, pageSize, total }}
          emptyMessage={isLoading ? 'Loading users…' : undefined}
        />
      )}
    </CollectionFrame>
  )
}
