import type { ReactNode } from 'react'
import {
  ArrowDown01Icon,
  ArrowUp01Icon,
  ArrowUpDownIcon,
} from '@hugeicons/core-free-icons'
import { HugeiconsIcon } from '@hugeicons/react'

import { Button } from '@/components/ui/button'
import {
  Pagination,
  PaginationContent,
  PaginationItem,
  PaginationNext,
  PaginationPrevious,
} from '@/components/ui/pagination'
import { NativeSelect, NativeSelectOption } from '@/components/ui/native-select'
import { TableHead } from '@/components/ui/table'
import { cn } from '@/lib/utils'

export type SortDirection = 'asc' | 'desc'

interface SortableTableHeadProps<TSort extends string> {
  children: ReactNode
  className?: string
  direction: SortDirection
  onSortChange: (sort: TSort, direction: SortDirection) => void
  sort: TSort
  sortKey: TSort
}

export function SortableTableHead<TSort extends string>({
  children,
  className,
  direction,
  onSortChange,
  sort,
  sortKey,
}: SortableTableHeadProps<TSort>) {
  const active = sort === sortKey
  const nextDirection: SortDirection =
    active && direction === 'asc' ? 'desc' : 'asc'
  const icon = active
    ? direction === 'asc'
      ? ArrowUp01Icon
      : ArrowDown01Icon
    : ArrowUpDownIcon

  return (
    <TableHead
      aria-sort={
        active ? (direction === 'asc' ? 'ascending' : 'descending') : 'none'
      }
      className={className}
    >
      <Button
        variant="ghost"
        size="sm"
        className="-ml-2 h-8"
        onClick={() => onSortChange(sortKey, nextDirection)}
      >
        {children}
        <HugeiconsIcon icon={icon} aria-hidden />
      </Button>
    </TableHead>
  )
}

const PAGE_SIZE_LABELS: Record<string, string> = {
  '20': '20 per page',
  '50': '50 per page',
  '100': '100 per page',
}

interface CollectionPaginationProps {
  className?: string
  onPageChange: (page: number) => void
  onPageSizeChange: (pageSize: number) => void
  page: number
  pageSize: number
  total: number
}

export function CollectionPagination({
  className,
  onPageChange,
  onPageSizeChange,
  page,
  pageSize,
  total,
}: CollectionPaginationProps) {
  const totalPages = Math.max(1, Math.ceil(total / pageSize))
  const start = total === 0 ? 0 : (page - 1) * pageSize + 1
  const end = Math.min(page * pageSize, total)

  return (
    <div
      className={cn(
        'flex flex-col gap-3 border-t pt-4 sm:flex-row sm:items-center sm:justify-between',
        className,
      )}
    >
      <p className="text-xs text-muted-foreground" aria-live="polite">
        {total === 0
          ? 'No results'
          : `Showing ${start}–${end} of ${total} results`}
      </p>

      <div className="flex items-center justify-between gap-3 sm:justify-end">
        <NativeSelect
          className="w-32"
          aria-label="Results per page"
          value={String(pageSize)}
          onChange={(event) => onPageSizeChange(Number(event.target.value))}
        >
          {Object.entries(PAGE_SIZE_LABELS).map(([value, label]) => (
            <NativeSelectOption key={value} value={value}>
              {label}
            </NativeSelectOption>
          ))}
        </NativeSelect>

        <Pagination className="w-auto">
          <PaginationContent>
            <PaginationItem>
              <PaginationPrevious
                href="#"
                onClick={(event) => {
                  event.preventDefault()
                  if (page > 1) onPageChange(page - 1)
                }}
                aria-disabled={page <= 1}
                className={page <= 1 ? 'pointer-events-none opacity-50' : ''}
              />
            </PaginationItem>
            <li className="min-w-20 text-center text-xs text-muted-foreground">
              Page {page} of {totalPages}
            </li>
            <PaginationItem>
              <PaginationNext
                href="#"
                onClick={(event) => {
                  event.preventDefault()
                  if (page < totalPages) onPageChange(page + 1)
                }}
                aria-disabled={page >= totalPages}
                className={
                  page >= totalPages ? 'pointer-events-none opacity-50' : ''
                }
              />
            </PaginationItem>
          </PaginationContent>
        </Pagination>
      </div>
    </div>
  )
}
