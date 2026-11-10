import { useMemo } from 'react'
import { encode } from 'uqr'
import { cn } from '@/lib/utils'

/** The dark modules of a QR code as one SVG path, one unit per module. */
export function qrCodePath(value: string) {
  const { data, size } = encode(value, { border: 2, ecc: 'M' })
  let path = ''
  data.forEach((row, y) => {
    row.forEach((dark, x) => {
      if (dark) path += `M${x} ${y}h1v1h-1z`
    })
  })
  return { size, path }
}

/**
 * A scan-to-open code for a link. It is always dark on white, whatever the
 * theme, because phone cameras read that most reliably.
 */
export function QrCode({
  value,
  label,
  className,
}: {
  value: string
  /** What scanning it does, for screen readers. */
  label: string
  className?: string
}) {
  const { size, path } = useMemo(() => qrCodePath(value), [value])
  return (
    <svg
      role="img"
      aria-label={label}
      viewBox={`0 0 ${size} ${size}`}
      className={cn('size-36 rounded-md bg-white', className)}
    >
      <path d={path} fill="black" />
    </svg>
  )
}
