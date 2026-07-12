import { createFileRoute } from '@tanstack/react-router'

import { BuildDetailRoute } from '@/components/build-details/build-detail-route'
import {
  getActiveInstanceOrRedirect,
  requireAuthOrRedirect,
} from '@/lib/instance-context'

export const Route = createFileRoute('/builds/$buildId')({
  staticData: { breadcrumbLabel: 'Details' },
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireAuthOrRedirect(instance.id)
  },
  component: BuildDetailRoute,
})
