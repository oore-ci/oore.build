import { createFileRoute } from '@tanstack/react-router'

import { BuildDetailRoute } from '@/components/build-details/build-detail-route'
import {
  getActiveInstanceOrRedirect,
  requireAuthOrRedirect,
} from '@/lib/instance-context'

export const Route = createFileRoute('/builds/$buildId')({
  staticData: {
    breadcrumbLabel: 'Details',
    breadcrumbParent: { label: 'Builds', to: '/builds' },
  },
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireAuthOrRedirect(instance.id)
  },
  validateSearch: (search: Record<string, unknown>): { install?: string } => ({
    install: typeof search.install === 'string' ? search.install : undefined,
  }),
  component: BuildDetailRoute,
})
