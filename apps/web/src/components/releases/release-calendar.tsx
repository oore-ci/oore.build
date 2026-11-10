import { useState } from 'react'
import { Link } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import { ArrowLeft01Icon, ArrowRight01Icon } from '@hugeicons/core-free-icons'
import type { Release } from '@oore/client/models'

import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Row } from '@/components/layout/row'
import { releaseDestinationLabel, releaseVersionLabel } from '@/lib/releases'
import { cn } from '@/lib/utils'

const WEEKDAYS = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun']

function dayKey(date: Date) {
  return `${date.getFullYear()}-${date.getMonth()}-${date.getDate()}`
}

/** The days of a month's grid, Monday first, padded to whole weeks. */
export function monthGrid(year: number, month: number): Array<Date | null> {
  const first = new Date(year, month, 1)
  const days = new Date(year, month + 1, 0).getDate()
  const lead = (first.getDay() + 6) % 7
  const cells: Array<Date | null> = Array.from({ length: lead }, () => null)
  for (let day = 1; day <= days; day += 1) {
    cells.push(new Date(year, month, day))
  }
  while (cells.length % 7) cells.push(null)
  return cells
}

/** Releases by the day they started, for a month at a glance. */
export function ReleaseCalendar({ releases }: { releases: Array<Release> }) {
  const today = new Date()
  const [shown, setShown] = useState({
    year: today.getFullYear(),
    month: today.getMonth(),
  })
  const byDay = new Map<string, Array<Release>>()
  for (const release of releases) {
    const key = dayKey(new Date(release.created_at * 1000))
    byDay.set(key, [...(byDay.get(key) ?? []), release])
  }
  const title = new Date(shown.year, shown.month, 1).toLocaleDateString(
    undefined,
    { month: 'long', year: 'numeric' },
  )
  const move = (delta: number) =>
    setShown(({ year, month }) => {
      const next = new Date(year, month + delta, 1)
      return { year: next.getFullYear(), month: next.getMonth() }
    })

  return (
    <Card size="sm">
      <CardHeader>
        <Row gap={2} className="justify-between">
          <CardTitle>Calendar</CardTitle>
          <Row gap={1}>
            <Button
              size="icon-sm"
              variant="ghost"
              aria-label="Previous month"
              onClick={() => move(-1)}
            >
              <HugeiconsIcon icon={ArrowLeft01Icon} size={16} />
            </Button>
            <span className="min-w-32 text-center text-sm font-medium">
              {title}
            </span>
            <Button
              size="icon-sm"
              variant="ghost"
              aria-label="Next month"
              onClick={() => move(1)}
            >
              <HugeiconsIcon icon={ArrowRight01Icon} size={16} />
            </Button>
          </Row>
        </Row>
      </CardHeader>
      <CardContent>
        <div
          className="grid grid-cols-7 gap-px overflow-hidden rounded-lg border bg-border text-xs"
          role="grid"
          aria-label={`Releases in ${title}`}
        >
          {WEEKDAYS.map((weekday) => (
            <div
              key={weekday}
              role="columnheader"
              className="bg-muted/50 px-1.5 py-1 text-muted-foreground"
            >
              {weekday}
            </div>
          ))}
          {monthGrid(shown.year, shown.month).map((date, index) => {
            const dayReleases = date ? (byDay.get(dayKey(date)) ?? []) : []
            const isToday = date ? dayKey(date) === dayKey(today) : false
            return (
              <div
                key={date ? dayKey(date) : `pad-${index}`}
                role="gridcell"
                className={cn(
                  'flex min-h-16 min-w-0 flex-col gap-0.5 bg-card p-1',
                  !date && 'bg-muted/30',
                )}
              >
                {date ? (
                  <span
                    className={cn(
                      'text-muted-foreground tabular-nums',
                      isToday && 'font-semibold text-foreground',
                    )}
                  >
                    {date.getDate()}
                  </span>
                ) : null}
                {dayReleases.slice(0, 3).map((release) => (
                  <Link
                    key={release.id}
                    to="/releases/$releaseId"
                    params={{ releaseId: release.id }}
                    title={`${releaseVersionLabel(release)} · ${releaseDestinationLabel(release)}`}
                    className="truncate rounded bg-muted px-1 hover:bg-muted/70"
                  >
                    {release.version_name ?? releaseVersionLabel(release)}{' '}
                    <span className="text-muted-foreground">
                      {release.store === 'app_store' ? 'iOS' : 'Android'}
                    </span>
                  </Link>
                ))}
                {dayReleases.length > 3 ? (
                  <span className="text-muted-foreground">
                    +{dayReleases.length - 3} more
                  </span>
                ) : null}
              </div>
            )
          })}
        </div>
      </CardContent>
    </Card>
  )
}
