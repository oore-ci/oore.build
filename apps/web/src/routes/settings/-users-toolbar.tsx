import {
  ArrowDown as ArrowDown01Icon,
  ArrowUp as ArrowUp01Icon,
} from 'lucide-react'
import { DynamicLucideIcon } from '@/components/ui/dynamic-lucide-icon'
import type { Table } from '@tanstack/react-table'

import { CollectionSearchInput } from '@/components/collection-search-input'
import type { SortDirection } from '@/components/collection-controls'
import { Button } from '@/components/ui/button'
import { NativeSelect, NativeSelectOption } from '@/components/ui/native-select'
import type { User } from '@/lib/types'
import type { UserSort } from './users'

const SORT_LABELS: Record<UserSort, string> = {
  created_at: 'Joined',
  email: 'Email',
  role: 'Role',
  status: 'Status',
}

interface UsersToolbarProps {
  direction: SortDirection
  initialSearch: string
  onBulkDisable: (userIds: Array<string>) => void
  onSearch: (value: string) => void
  onSortChange: (sort: UserSort, direction: SortDirection) => void
  sort: UserSort
  table: Table<User>
}

export function UsersToolbar({
  direction,
  initialSearch,
  onBulkDisable,
  onSearch,
  onSortChange,
  sort,
  table,
}: UsersToolbarProps) {
  const selectedRows = table.getFilteredSelectedRowModel().rows

  return (
    <div className="flex flex-col gap-3 sm:flex-row sm:items-center">
      <CollectionSearchInput
        initialValue={initialSearch}
        onSearch={onSearch}
        placeholder="Search users"
        ariaLabel="Search users"
      />

      <div className="grid grid-cols-[1fr_auto] gap-3 sm:hidden">
        <NativeSelect
          aria-label="Sort users"
          value={sort}
          onChange={(event) =>
            onSortChange(event.target.value as UserSort, direction)
          }
        >
          {Object.entries(SORT_LABELS).map(([sortValue, label]) => (
            <NativeSelectOption key={sortValue} value={sortValue}>
              {label}
            </NativeSelectOption>
          ))}
        </NativeSelect>
        <Button
          variant="outline"
          size="icon"
          aria-label={
            direction === 'asc' ? 'Sort descending' : 'Sort ascending'
          }
          title={direction === 'asc' ? 'Sort descending' : 'Sort ascending'}
          onClick={() =>
            onSortChange(sort, direction === 'asc' ? 'desc' : 'asc')
          }
        >
          <DynamicLucideIcon
            icon={direction === 'asc' ? ArrowUp01Icon : ArrowDown01Icon}
          />
        </Button>
      </div>

      {selectedRows.length > 0 ? (
        <div className="flex items-center justify-between gap-3 sm:ml-auto sm:justify-end">
          <span className="text-sm text-muted-foreground" aria-live="polite">
            {selectedRows.length} selected
          </span>
          <Button
            variant="destructive"
            size="sm"
            onClick={() =>
              onBulkDisable(selectedRows.map((row) => row.original.id))
            }
          >
            Disable selected
          </Button>
        </div>
      ) : null}
    </div>
  )
}
