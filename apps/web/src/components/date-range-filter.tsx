import { HugeiconsIcon } from '@hugeicons/react'
import { Calendar03Icon } from '@hugeicons/core-free-icons'
import { format } from 'date-fns'
import type { DateRange } from 'react-day-picker'
import * as z from 'zod'

import { Button } from '@/components/ui/button'
import { Calendar } from '@/components/ui/calendar'
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from '@/components/ui/popover'
import type { SearchValue } from '@/lib/search-input'

/** A `yyyy-MM-dd` search value, or undefined when it is not a real day. */
export function validSearchDate(value: SearchValue): string | undefined {
  const parsed = z.string().safeParse(value)
  if (!parsed.success || !/^\d{4}-\d{2}-\d{2}$/.test(parsed.data)) {
    return undefined
  }
  return Number.isNaN(new Date(`${parsed.data}T00:00:00`).getTime())
    ? undefined
    : parsed.data
}

/** Local midnight at the start of a `yyyy-MM-dd` day. */
export function dateFromSearch(value?: string): Date | undefined {
  if (!value) return undefined
  const date = new Date(`${value}T00:00:00`)
  return Number.isNaN(date.getTime()) ? undefined : date
}

/** The `from`/`to` search values for a picked range. */
export function searchDateRange(range: DateRange | undefined) {
  return {
    from: range?.from ? format(range.from, 'yyyy-MM-dd') : undefined,
    to: range?.to ? format(range.to, 'yyyy-MM-dd') : undefined,
  }
}

export function DateRangeFilter({
  from,
  onChange,
  placeholder = 'Pick a date range',
  to,
}: {
  from?: string
  onChange: (range: DateRange | undefined) => void
  placeholder?: string
  to?: string
}) {
  const fromDate = dateFromSearch(from)
  const toDate = dateFromSearch(to)
  const selected: DateRange | undefined =
    fromDate || toDate ? { from: fromDate, to: toDate } : undefined
  const label = fromDate
    ? toDate
      ? `${format(fromDate, 'MMM d, yyyy')} to ${format(toDate, 'MMM d, yyyy')}`
      : `From ${format(fromDate, 'MMM d, yyyy')}`
    : toDate
      ? `Through ${format(toDate, 'MMM d, yyyy')}`
      : placeholder

  return (
    <Popover>
      <PopoverTrigger
        render={
          <Button
            variant="secondary"
            className="max-w-64 justify-start overflow-hidden text-left font-normal"
            aria-label={`Date range: ${label}`}
          />
        }
      >
        <HugeiconsIcon
          icon={Calendar03Icon}
          data-icon="inline-start"
          aria-hidden
        />
        <span className="truncate">{label}</span>
      </PopoverTrigger>
      <PopoverContent
        align="end"
        className="max-h-[calc(100dvh-2rem)] w-auto max-w-[calc(100vw-2rem)] overflow-auto p-0"
      >
        <Calendar
          mode="range"
          selected={selected}
          defaultMonth={fromDate ?? toDate}
          numberOfMonths={2}
          onSelect={onChange}
        />
      </PopoverContent>
    </Popover>
  )
}

/**
 * Unix-second bounds for `from`/`to` days in the viewer's time zone: from
 * midnight starting `from` up to, not including, midnight after `to`.
 */
export function searchDateRangeBounds(from?: string, to?: string) {
  const start = dateFromSearch(from)
  const end = dateFromSearch(to)
  if (end) end.setDate(end.getDate() + 1)
  return {
    after: start ? Math.floor(start.getTime() / 1000) : undefined,
    before: end ? Math.floor(end.getTime() / 1000) : undefined,
  }
}
