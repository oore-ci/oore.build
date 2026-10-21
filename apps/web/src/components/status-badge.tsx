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
      {label}
    </Badge>
  )
}
