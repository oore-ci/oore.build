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
  ArrowDown02Icon,
  ArrowUp02Icon,
  ArrowUpDownIcon,
  FilterHorizontalIcon,
  LayoutTable01Icon,
  MoreHorizontalCircle01Icon,
} from '@hugeicons/core-free-icons'
import { HugeiconsIcon } from '@hugeicons/react'
import { useState, type ReactNode } from 'react'

import { Button } from '@/components/ui/button'
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { Input } from '@/components/ui/input'
import { PAGE_SEARCH } from '@/lib/keyboard-shortcuts'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import { formatEnumLabel } from '@/lib/format-utils'
import { cn } from '@/lib/utils'
import type { TableDensity } from '@/stores/table-density-store'
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

  // Zero the button's own inset so the label lines up with the cell values
  // below it; arrows, not chevrons, so it does not read as a dropdown.
  return (
    <Button
      variant="ghost"
      size="sm"
      className="-ml-3 px-3 has-data-[icon=inline-end]:pr-1.5"
      onClick={() => column.toggleSorting(direction === 'asc')}
    >
      {title}
      <HugeiconsIcon
        icon={
          direction === 'asc'
            ? ArrowUp02Icon
            : direction === 'desc'
              ? ArrowDown02Icon
              : ArrowUpDownIcon
        }
        data-icon="inline-end"
        className={direction ? undefined : 'text-muted-foreground'}
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

// The first data column stays pinned while the table scrolls sideways so each
// row keeps its identity in view. A leading selection column scrolls away.
function stickyColumnId(columnIds: Array<string>) {
  return columnIds.find((id) => id !== 'select')
}

// Tighter than the primitive's px-4 between columns so dense tables such as
// Builds fit an 1100px window, with the outer edges kept at px-4.
const CELL_INSET = 'px-3 first:pl-4 last:pr-4'
// Compact rows trade vertical padding for more rows in view.
const COMPACT_CELL = 'in-data-[density=compact]:py-0.5'
// The trailing row-actions column hugs its trigger.
const ACTIONS_CELL = 'w-px pl-0'

const STICKY_CELL =
  'sticky left-0 z-10 bg-background in-data-[slot=card]:bg-card in-data-[scrolled=true]:shadow-[inset_-1px_0_0_var(--color-border)]'
const STICKY_BODY_CELL = cn(
  STICKY_CELL,
  'group-hover/row:bg-[color-mix(in_oklab,var(--color-muted)_50%,var(--color-background))] group-data-[state=selected]/row:bg-muted',
)

export function DataTable<TData extends RowData>({
  actions,
  activeFilterCount = 0,
  collapseFilters = false,
  density,
  emptyMessage,
  filters,
  isLoading = false,
  onDensityChange,
  onClearFilters,
  pagination,
  search,
  table,
}: {
  /** Controls that stay visible at every width, such as bulk or create actions. */
  actions?: ReactNode
  /** Filters in effect, shown on the compact Filters disclosure. */
  activeFilterCount?: number
  /**
   * Keep filters in the Filters disclosure at every width, for tables with
   * more filters than one toolbar row holds. It opens while any apply.
   */
  collapseFilters?: boolean
  /** Compact rows tighten padding; offered in the Columns menu. */
  density?: TableDensity
  emptyMessage?: ReactNode
  /** Collapse into a Filters disclosure when the toolbar is narrow. */
  filters?: ReactNode
  /** Hides the clear actions while the empty row only means loading. */
  isLoading?: boolean
  /** Pass while search or filters narrow the results. */
  onClearFilters?: () => void
  onDensityChange?: (density: TableDensity) => void
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
  const [filtersOpen, setFiltersOpen] = useState(
    collapseFilters && activeFilterCount > 0,
  )
  // Collapsed filters open when some start to apply, such as on navigation.
  const [seenFilterCount, setSeenFilterCount] = useState(activeFilterCount)
  if (seenFilterCount !== activeFilterCount) {
    setSeenFilterCount(activeFilterCount)
    if (collapseFilters && seenFilterCount === 0 && activeFilterCount > 0) {
      setFiltersOpen(true)
    }
  }
  const visibleColumns = table.getVisibleLeafColumns()
  const stickyId = stickyColumnId(visibleColumns.map((column) => column.id))
  const rows = table.getRowModel().rows
  const hideableColumns = table
    .getAllColumns()
    .filter((column) => column.getCanHide())
  // Embedded tables (recent builds, members, endpoints) pass no controls and
  // get no toolbar; a lone Columns menu is not a toolbar.
  const hasToolbar = Boolean(search || filters || actions)
  const clearFiltersButton = onClearFilters ? (
    <Button variant="ghost" onClick={onClearFilters}>
      Clear filters
    </Button>
  ) : null
  // Search-only tables still get a way out of an empty result.
  const clearSearch =
    !isLoading && !onClearFilters && search?.value
      ? () => search.onChange('')
      : undefined
  const emptyText =
    emptyMessage ??
    (search?.value ? `No results for “${search.value}”.` : 'No results.')
  const pageCount = pagination
    ? Math.max(1, Math.ceil(pagination.total / pagination.pageSize))
    : 1
  const firstResult = pagination
    ? (pagination.page - 1) * pagination.pageSize + 1
    : 0
  const lastResult = pagination
    ? Math.min(pagination.page * pagination.pageSize, pagination.total)
    : 0

  return (
    <div className="@container w-full">
      {hasToolbar ? (
        <div className="flex flex-col gap-2 py-4">
          <div className="flex items-center gap-2">
            {search ? (
              <Input
                type="search"
                {...PAGE_SEARCH}
                value={search.value}
                placeholder={search.placeholder}
                aria-label={search.placeholder}
                onChange={(event) => search.onChange(event.target.value)}
                className="min-w-0 flex-1 @2xl:max-w-sm"
              />
            ) : null}
            <div className="ml-auto flex shrink-0 items-center gap-2">
              {filters ? (
                <>
                  {collapseFilters ? null : (
                    <div className="hidden items-center gap-2 @2xl:flex">
                      {clearFiltersButton}
                      {filters}
                    </div>
                  )}
                  <Button
                    variant={filtersOpen ? 'outline' : 'secondary'}
                    className={collapseFilters ? undefined : '@2xl:hidden'}
                    aria-expanded={filtersOpen}
                    onClick={() => setFiltersOpen((open) => !open)}
                  >
                    <HugeiconsIcon
                      icon={FilterHorizontalIcon}
                      data-icon="inline-start"
                      aria-hidden
                    />
                    Filters
                    {activeFilterCount > 0 ? (
                      <span className="rounded-full bg-primary px-1.5 text-xs text-primary-foreground tabular-nums">
                        {activeFilterCount}
                      </span>
                    ) : null}
                  </Button>
                </>
              ) : (
                clearFiltersButton
              )}
              {actions}
              {hideableColumns.length > 0 || onDensityChange ? (
                <DropdownMenu>
                  <DropdownMenuTrigger
                    render={<Button variant="secondary" />}
                    aria-label="Columns"
                  >
                    <HugeiconsIcon
                      icon={LayoutTable01Icon}
                      className="@lg:hidden"
                      aria-hidden
                    />
                    <span className="hidden @lg:inline">Columns</span>
                    <HugeiconsIcon
                      icon={ArrowDown01Icon}
                      className="hidden @lg:block"
                      aria-hidden
                    />
                  </DropdownMenuTrigger>
                  <DropdownMenuContent align="end" className="w-44">
                    <DropdownMenuGroup>
                      {hideableColumns.map((column) => (
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
                    {onDensityChange ? (
                      <>
                        {hideableColumns.length > 0 ? (
                          <DropdownMenuSeparator />
                        ) : null}
                        <DropdownMenuGroup>
                          <DropdownMenuCheckboxItem
                            checked={density === 'compact'}
                            onCheckedChange={(value) =>
                              onDensityChange(value ? 'compact' : 'comfortable')
                            }
                          >
                            Compact rows
                          </DropdownMenuCheckboxItem>
                        </DropdownMenuGroup>
                      </>
                    ) : null}
                  </DropdownMenuContent>
                </DropdownMenu>
              ) : null}
            </div>
          </div>
          {filters && filtersOpen ? (
            <div
              className={cn(
                'flex flex-wrap items-center gap-2 rounded-md border p-2',
                !collapseFilters && '@2xl:hidden',
              )}
              role="group"
              aria-label="Filters"
            >
              {filters}
              {clearFiltersButton}
            </div>
          ) : null}
        </div>
      ) : null}
      <div
        className="overflow-hidden rounded-md border"
        data-density={density === 'compact' ? 'compact' : undefined}
      >
        <Table>
          <TableHeader>
            {table.getHeaderGroups().map((headerGroup) => (
              <TableRow key={headerGroup.id}>
                {headerGroup.headers.map((header) => {
                  const sorted = header.column.getCanSort()
                    ? dataTableColumnSortDirection(
                        table.options.state?.sorting,
                        header.column.id,
                        () => header.column.getIsSorted(),
                      )
                    : false
                  return (
                    <TableHead
                      key={header.id}
                      aria-sort={
                        sorted === 'asc'
                          ? 'ascending'
                          : sorted === 'desc'
                            ? 'descending'
                            : undefined
                      }
                      className={cn(
                        CELL_INSET,
                        header.column.id === 'actions' && ACTIONS_CELL,
                        header.column.id === stickyId && STICKY_CELL,
                      )}
                    >
                      {header.isPlaceholder ? null : (
                        <table.FlexRender header={header} />
                      )}
                    </TableHead>
                  )
                })}
              </TableRow>
            ))}
          </TableHeader>
          <TableBody>
            {rows.length ? (
              rows.map((row) => (
                <TableRow
                  key={row.id}
                  className="group/row"
                  data-state={row.getIsSelected() ? 'selected' : undefined}
                >
                  {row.getVisibleCells().map((cell) => (
                    <TableCell
                      key={cell.id}
                      className={cn(
                        CELL_INSET,
                        COMPACT_CELL,
                        cell.column.id === 'actions' && ACTIONS_CELL,
                        cell.column.id === stickyId && STICKY_BODY_CELL,
                      )}
                    >
                      <table.FlexRender cell={cell} />
                    </TableCell>
                  ))}
                </TableRow>
              ))
            ) : (
              <TableRow className="hover:bg-transparent">
                <TableCell
                  colSpan={visibleColumns.length}
                  className="h-32 text-center whitespace-normal text-muted-foreground"
                >
                  <div className="flex flex-col items-center justify-center gap-3">
                    <span>{emptyText}</span>
                    {isLoading ? null : onClearFilters ? (
                      <Button
                        variant="outline"
                        size="sm"
                        onClick={onClearFilters}
                      >
                        Clear filters
                      </Button>
                    ) : clearSearch ? (
                      <Button variant="outline" size="sm" onClick={clearSearch}>
                        Clear search
                      </Button>
                    ) : null}
                  </div>
                </TableCell>
              </TableRow>
            )}
          </TableBody>
        </Table>
      </div>
      {pagination && pagination.total > 0 ? (
        <div className="flex items-center justify-between gap-2 py-4">
          <p className="text-sm text-muted-foreground tabular-nums">
            {firstResult}–{lastResult} of {pagination.total}
          </p>
          {pageCount > 1 ? (
            <div className="flex items-center gap-2">
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
                disabled={pagination.page >= pageCount}
              >
                Next
              </Button>
            </div>
          ) : null}
        </div>
      ) : null}
    </div>
  )
}

/**
 * The row actions trigger: an icon-size ghost button named for its row, so a
 * screen reader hears "Actions for build #12" rather than a generic menu, and
 * at least 44px square on touch and compact screens.
 */
export function DataTableRowActionsTrigger({ label }: { label: string }) {
  return (
    <DropdownMenuTrigger
      render={
        <Button
          variant="ghost"
          size="icon-sm"
          className="max-sm:size-11 pointer-coarse:size-11"
          aria-label={label}
        />
      }
    >
      <HugeiconsIcon icon={MoreHorizontalCircle01Icon} aria-hidden />
    </DropdownMenuTrigger>
  )
}
