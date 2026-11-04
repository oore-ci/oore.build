import { HugeiconsIcon } from '@hugeicons/react'
import { WorkflowSquare03Icon } from '@hugeicons/core-free-icons'

import { Badge } from '@/components/ui/badge'

/** The `.oore.yaml` workflow a build ran, beside its number or status. */
export function WorkflowBadge({
  workflow,
  className,
}: {
  workflow?: string | null
  className?: string
}) {
  if (!workflow) return null
  return (
    <Badge variant="outline" className={className} title="Workflow">
      <HugeiconsIcon
        icon={WorkflowSquare03Icon}
        strokeWidth={2}
        data-icon="inline-start"
        aria-hidden
      />
      <span className="sr-only">Workflow </span>
      {workflow}
    </Badge>
  )
}
