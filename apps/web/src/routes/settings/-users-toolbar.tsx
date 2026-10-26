import type { DataTableInstance } from '@/components/data-table'
import { Button } from '@/components/ui/button'
import type { User } from '@oore/client/models'
import { Row } from '@/components/layout/row'
import { textVariants } from '@/lib/recipes'

interface UsersToolbarProps {
  onBulkDisable: (userIds: Array<string>) => void
  onGrantAccess: (users: Array<User>) => void
  table: DataTableInstance<User>
}

export function UsersToolbar({
  onBulkDisable,
  onGrantAccess,
  table,
}: UsersToolbarProps) {
  const selectedRows = table.getFilteredSelectedRowModel().rows
  if (selectedRows.length === 0) return null

  return (
    <Row gap={3}>
      <span className={textVariants({ variant: 'muted' })} aria-live="polite">
        {selectedRows.length} selected
      </span>
      <Button
        variant="outline"
        size="sm"
        onClick={() => onGrantAccess(selectedRows.map((row) => row.original))}
      >
        Give project access
      </Button>
      <Button
        variant="destructive"
        size="sm"
        onClick={() =>
          onBulkDisable(selectedRows.map((row) => row.original.id))
        }
      >
        Disable selected
      </Button>
    </Row>
  )
}
