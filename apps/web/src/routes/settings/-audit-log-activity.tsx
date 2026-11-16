import type { AuditLogActivityResponse } from '@oore/client/models'

import { Skeleton } from '@/components/ui/skeleton'
import { Text } from '@/components/layout/text'
import { cn } from '@/lib/utils'

const BAR_HEIGHT = 40
const BAR_CLASS =
  'w-full rounded-xs bg-muted-foreground/40 group-hover:bg-muted-foreground group-aria-pressed:bg-foreground'
const DAY_BUTTON_CLASS =
  'group flex h-10 min-w-1.5 flex-1 items-end rounded-xs outline-none focus-visible:ring-[3px] focus-visible:ring-ring/50'

function Stat({ label, value }: { label: string; value: number }) {
  return (
    <div>
      <p className="text-xl font-medium tabular-nums">{value}</p>
      <Text variant="caption">{label}</Text>
    </div>
  )
}

function dayLabel(date: string) {
  return new Date(`${date}T00:00:00`).toLocaleDateString(undefined, {
    day: 'numeric',
    month: 'short',
  })
}

/**
 * How busy the instance has been: entries per day, people active, and what
 * was blocked or failed. Picking a day shows that day's entries.
 */
const STRIP =
  'flex flex-col gap-4 rounded-md border px-5 py-4 sm:flex-row sm:items-end sm:justify-between'

export function AuditLogActivity({
  activity,
  onSelectDay,
  selectedDay,
}: {
  activity: AuditLogActivityResponse | undefined
  onSelectDay: (date: string | undefined) => void
  /** The day the list is narrowed to, when it is exactly one day. */
  selectedDay: string | undefined
}) {
  if (!activity) return <Skeleton className="h-24 w-full rounded-md" />
  const { days } = activity
  const peak = Math.max(1, ...days.map((day) => day.count))
  const span = days.length

  return (
    <section aria-label={`Activity in the last ${span} days`} className={STRIP}>
      <div className="flex gap-8">
        <Stat
          label={`${activity.total === 1 ? 'event' : 'events'} in the last ${span} days`}
          value={activity.total}
        />
        <Stat
          label={activity.actors === 1 ? 'person active' : 'people active'}
          value={activity.actors}
        />
        <Stat label="blocked or failed" value={activity.problems} />
      </div>
      <div className="flex w-full flex-col gap-1.5 sm:max-w-md">
        <div className="flex items-end gap-1">
          {days.map((day) => {
            const selected = day.date === selectedDay
            const events = `${day.count} ${day.count === 1 ? 'event' : 'events'}`
            return (
              <button
                key={day.date}
                type="button"
                className={DAY_BUTTON_CLASS}
                aria-pressed={selected}
                aria-label={`${dayLabel(day.date)}, ${events}`}
                title={`${dayLabel(day.date)}: ${events}`}
                onClick={() => onSelectDay(selected ? undefined : day.date)}
              >
                <span
                  className={cn(BAR_CLASS, day.count === 0 && 'opacity-50')}
                  style={{
                    height: Math.max(2, (day.count / peak) * BAR_HEIGHT),
                  }}
                />
              </button>
            )
          })}
        </div>
        <div className="flex justify-between">
          <Text variant="caption">{days[0] ? dayLabel(days[0].date) : ''}</Text>
          <Text variant="caption">Today</Text>
        </div>
      </div>
    </section>
  )
}
