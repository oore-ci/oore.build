import { formatAbsoluteTime, relativeTime } from '@/lib/format-utils'

/**
 * Relative timestamp with the absolute value on hover, so lists read at a
 * glance and the exact moment stays one hover away.
 */
export function RelativeTime({
  at,
  className,
}: {
  at: number
  className?: string
}) {
  return (
    <time
      className={className}
      dateTime={new Date(at * 1000).toISOString()}
      title={formatAbsoluteTime(at)}
    >
      {relativeTime(at)}
    </time>
  )
}
