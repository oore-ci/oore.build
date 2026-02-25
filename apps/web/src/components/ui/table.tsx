import type { JSX } from 'solid-js'
import { cn } from '@/lib/utils'

interface TableProps extends JSX.HTMLAttributes<HTMLTableElement> {
  class?: string
  children?: JSX.Element
}

export function Table(props: TableProps) {
  const { class: className, ...rest } = props
  return (
    <div data-slot="table-container" class="relative w-full overflow-x-auto">
      <table
        data-slot="table"
        class={cn('w-full caption-bottom text-sm', className)}
        {...rest}
      />
    </div>
  )
}

interface TableSectionProps extends JSX.HTMLAttributes<HTMLTableSectionElement> {
  class?: string
  children?: JSX.Element
}

export function TableHeader(props: TableSectionProps) {
  const { class: className, ...rest } = props
  return (
    <thead
      data-slot="table-header"
      class={cn('[&_tr]:border-b', className)}
      {...rest}
    />
  )
}

export function TableBody(props: TableSectionProps) {
  const { class: className, ...rest } = props
  return (
    <tbody
      data-slot="table-body"
      class={cn('[&_tr:last-child]:border-0', className)}
      {...rest}
    />
  )
}

export function TableFooter(props: TableSectionProps) {
  const { class: className, ...rest } = props
  return (
    <tfoot
      data-slot="table-footer"
      class={cn(
        'bg-muted/50 border-t font-medium [&>tr]:last:border-b-0',
        className,
      )}
      {...rest}
    />
  )
}

interface RowProps extends JSX.HTMLAttributes<HTMLTableRowElement> {
  class?: string
  children?: JSX.Element
}

export function TableRow(props: RowProps) {
  const { class: className, ...rest } = props
  return (
    <tr
      data-slot="table-row"
      class={cn(
        'hover:bg-muted/50 data-[state=selected]:bg-muted border-b transition-colors',
        className,
      )}
      {...rest}
    />
  )
}

interface CellProps extends JSX.HTMLAttributes<HTMLTableCellElement> {
  class?: string
  children?: JSX.Element
}

export function TableHead(props: CellProps) {
  const { class: className, ...rest } = props
  return (
    <th
      data-slot="table-head"
      class={cn(
        'text-foreground h-10 px-2 text-left align-middle font-medium whitespace-nowrap [&:has([role=checkbox])]:pr-0',
        className,
      )}
      {...rest}
    />
  )
}

export function TableCell(props: CellProps) {
  const { class: className, ...rest } = props
  return (
    <td
      data-slot="table-cell"
      class={cn(
        'p-2 align-middle whitespace-nowrap [&:has([role=checkbox])]:pr-0',
        className,
      )}
      {...rest}
    />
  )
}

interface TableCaptionProps extends JSX.HTMLAttributes<HTMLTableCaptionElement> {
  class?: string
  children?: JSX.Element
}

export function TableCaption(props: TableCaptionProps) {
  const { class: className, ...rest } = props
  return (
    <caption
      data-slot="table-caption"
      class={cn('text-muted-foreground mt-4 text-sm', className)}
      {...rest}
    />
  )
}
