import {
  useTable,
  type CellData,
  type Column,
  type ColumnDef,
  type ReactTable,
  type RowData,
  type TableOptions,
} from '@tanstack/react-table'
import {
  ArrowDown01Icon,
  ArrowUp01Icon,
  ArrowUpDownIcon,
} from '@hugeicons/core-free-icons'
import { HugeiconsIcon } from '@hugeicons/react'
import type { ReactNode } from 'react'

import { Button } from '@/components/ui/button'
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { Input } from '@/components/ui/input'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import { formatEnumLabel } from '@/lib/format-utils'
import {
  dataTableColumnSortDirection,
  features,
  type DataTableFeatures,
} from './data-table-features'

export type { DataTableFeatures } from './data-table-features'

export type DataTableColumnDef<TData extends RowData> = ColumnDef<
  DataTableFeatures,
  TData
>
export type DataTableInstance<TData extends RowData> = ReactTable<
  DataTableFeatures,
  TData
>

type DataTableOptions<TData extends RowData> = Omit<
  TableOptions<DataTableFeatures, TData>,
  'columns' | 'data' | 'features'
> & {
  columns: Array<DataTableColumnDef<TData>>
  data: Array<TData>
}

export function useDataTable<TData extends RowData>({
  columns,
  data,
  ...options
}: DataTableOptions<TData>) {
  return useTable({
    features,
    columns,
    data,
    manualFiltering: true,
    manualPagination: true,
    manualSorting: true,
    ...options,
  })
}

export function DataTableColumnHeader<
  TData extends RowData,
  TValue extends CellData,
>({
  column,
  title,
}: {
  column: Column<DataTableFeatures, TData, TValue>
  title: string
}) {
  if (!column.getCanSort()) return title

  const direction = dataTableColumnSortDirection(
    column.table.options.state?.sorting,
    column.id,
    () => column.getIsSorted(),
  )

  return (
    <Button
      variant="ghost"
      onClick={() => column.toggleSorting(direction === 'asc')}
    >
      {title}
      <HugeiconsIcon
        icon={
          direction === 'asc'
            ? ArrowUp01Icon
            : direction === 'desc'
              ? ArrowDown01Icon
              : ArrowUpDownIcon
        }
        aria-hidden
      />
    </Button>
  )
}

export function DataTableSelectFilter({
  onValueChange,
  options,
  value,
}: {
  onValueChange: (value: string) => void
  options: Readonly<Record<string, string>>
  value: string
}) {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger render={<Button variant="secondary" />}>
        {options[value] ?? value}
        <HugeiconsIcon icon={ArrowDown01Icon} aria-hidden />
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-44">
        <DropdownMenuRadioGroup value={value} onValueChange={onValueChange}>
          {Object.entries(options).map(([optionValue, label]) => (
            <DropdownMenuRadioItem key={optionValue} value={optionValue}>
              {label}
            </DropdownMenuRadioItem>
          ))}
        </DropdownMenuRadioGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}

export function DataTable<TData extends RowData>({
  emptyMessage = 'No results.',
  filters,
  pagination,
  search,
  table,
}: {
  emptyMessage?: string
  filters?: ReactNode
  pagination?: {
    onPageChange: (page: number) => void
    page: number
    pageSize: number
    total: number
  }
  search?: {
    onChange: (value: string) => void
    placeholder: string
    value: string
  }
  table: DataTableInstance<TData>
}) {
  const visibleColumns = table.getVisibleLeafColumns()
  const rows = table.getRowModel().rows
  // Embedded tables (recent builds, members, endpoints) pass no controls and
  // get no toolbar; a lone Columns menu is not a toolbar.
  const hasToolbar = Boolean(search || filters)

  return (
    <div className="w-full">
      {hasToolbar ? (
        <div className="flex flex-wrap items-center gap-2 py-4">
          {search ? (
            <Input
              value={search.value}
              placeholder={search.placeholder}
              onChange={(event) => search.onChange(event.target.value)}
              className="max-w-sm"
            />
          ) : null}
          <div className="ml-auto flex flex-wrap items-center justify-end gap-2">
            {filters}
            <DropdownMenu>
              <DropdownMenuTrigger render={<Button variant="secondary" />}>
                Columns
                <HugeiconsIcon icon={ArrowDown01Icon} aria-hidden />
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end" className="w-44">
                <DropdownMenuGroup>
                  {table
                    .getAllColumns()
                    .filter((column) => column.getCanHide())
                    .map((column) => (
                      <DropdownMenuCheckboxItem
                        key={column.id}
                        checked={column.getIsVisible()}
                        onCheckedChange={(value) =>
                          column.toggleVisibility(!!value)
                        }
                      >
                        {formatEnumLabel(column.id)}
                      </DropdownMenuCheckboxItem>
                    ))}
                </DropdownMenuGroup>
              </DropdownMenuContent>
            </DropdownMenu>
          </div>
        </div>
      ) : null}
      <div className="overflow-hidden rounded-md border">
        <Table>
          <TableHeader>
            {table.getHeaderGroups().map((headerGroup) => (
              <TableRow key={headerGroup.id}>
                {headerGroup.headers.map((header) => (
                  <TableHead key={header.id}>
                    {header.isPlaceholder ? null : (
                      <table.FlexRender header={header} />
                    )}
                  </TableHead>
                ))}
              </TableRow>
            ))}
          </TableHeader>
          <TableBody>
            {rows.length ? (
              rows.map((row) => (
                <TableRow
                  key={row.id}
                  data-state={row.getIsSelected() ? 'selected' : undefined}
                >
                  {row.getVisibleCells().map((cell) => (
                    <TableCell key={cell.id}>
                      <table.FlexRender cell={cell} />
                    </TableCell>
                  ))}
                </TableRow>
              ))
            ) : (
              <TableRow>
                <TableCell
                  colSpan={visibleColumns.length}
                  className="h-24 text-center"
                >
                  {emptyMessage}
                </TableCell>
              </TableRow>
            )}
          </TableBody>
        </Table>
      </div>
      {pagination ? (
        <div className="flex items-center justify-end gap-2 py-4">
          <Button
            variant="outline"
            size="sm"
            onClick={() => pagination.onPageChange(pagination.page - 1)}
            disabled={pagination.page <= 1}
          >
            Previous
          </Button>
          <Button
            variant="outline"
            size="sm"
            onClick={() => pagination.onPageChange(pagination.page + 1)}
            disabled={pagination.page * pagination.pageSize >= pagination.total}
          >
            Next
          </Button>
        </div>
      ) : null}
    </div>
  )
}
