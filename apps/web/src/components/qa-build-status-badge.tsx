import { HugeiconsIcon } from '@hugeicons/react'
import { CheckmarkCircle02Icon } from '@hugeicons/core-free-icons'

import type { BuildStatus } from '@oore/client/models'
import { StatusBadge } from '@/components/status-badge'
import { Badge } from '@/components/ui/badge'

/** One status vocabulary for the QA hub and install detail. */
export default function QaBuildStatusBadge({
  status,
}: {
  status: BuildStatus
}) {
  if (status === 'succeeded') {
    return (
      <Badge variant="success">
        <HugeiconsIcon icon={CheckmarkCircle02Icon} />
        Ready to test
      </Badge>
    )
  }
  return <StatusBadge kind="build" value={status} />
}
