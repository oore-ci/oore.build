import type { Table } from '@tanstack/react-table'

import type { User } from '@/lib/types'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'

interface UsersToolbarProps {
  table: Table<User>
  onBulkDisable: (userIds: Array<string>) => void
}

export function UsersToolbar({ table, onBulkDisable }: UsersToolbarProps) {
  const selectedCount = table.getFilteredSelectedRowModel().rows.length

  const handleBulkDisable = () => {
    const ids = table
      .getFilteredSelectedRowModel()
      .rows.map((row) => row.original.id)
    onBulkDisable(ids)
  }

  return (
    <div className="flex items-center gap-3">
      <Input
        placeholder="Filter by email..."
        value={
          (table.getColumn('email')?.getFilterValue() as string | undefined) ??
          ''
        }
        onChange={(e) =>
          table.getColumn('email')?.setFilterValue(e.target.value)
        }
        className="max-w-xs"
      />
      <div className="ml-auto flex items-center gap-3">
        {selectedCount > 0 ? (
          <>
            <span className="text-sm text-muted-foreground">
              {selectedCount} selected
            </span>
            <Button variant="destructive" size="sm" onClick={handleBulkDisable}>
              Disable selected
            </Button>
          </>
        ) : null}
      </div>
    </div>
  )
}
