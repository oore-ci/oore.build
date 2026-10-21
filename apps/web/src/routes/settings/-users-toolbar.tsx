import type { DataTableInstance } from '@/components/data-table'
import { Button } from '@/components/ui/button'
import type { User } from '@oore/client/models'

interface UsersToolbarProps {
  onBulkDisable: (userIds: Array<string>) => void
  table: DataTableInstance<User>
}

export function UsersToolbar({ onBulkDisable, table }: UsersToolbarProps) {
  const selectedRows = table.getFilteredSelectedRowModel().rows
  if (selectedRows.length === 0) return null

  return (
    <div className="flex items-center gap-3">
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
  )
}
