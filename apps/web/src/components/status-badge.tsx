import { Badge } from '@/components/ui/badge'
import { getStatusPresentation, type StatusKind } from '@/lib/status-variants'

/**
 * The one way to show a machine status in the product. Routes pass the raw
 * enum value; label casing and semantic color come from `status-variants`.
 */
export function StatusBadge({
  className,
  kind,
  value,
}: {
  className?: string
  kind: StatusKind
  value: string
}) {
  const { label, variant } = getStatusPresentation(kind, value)
  return (
    <Badge variant={variant} className={className}>
      {kind === 'build' && value === 'running' ? (
        <span aria-hidden className="relative flex size-1.5">
          <span className="absolute inline-flex size-full rounded-full bg-current opacity-60 motion-safe:animate-ping" />
          <span className="relative inline-flex size-1.5 rounded-full bg-current" />
        </span>
      ) : null}
      {label}
    </Badge>
  )
}
