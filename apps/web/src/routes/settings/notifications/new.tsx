import { createFileRoute } from '@tanstack/react-router'

import {
  getActiveInstanceOrRedirect,
  requireAuthOrRedirect,
} from '@/lib/instance-context'

export const Route = createFileRoute('/settings/notifications/new')({
  staticData: {
    breadcrumbLabel: 'New Channel',
    breadcrumbParent: {
      label: 'Notifications',
      to: '/settings/notifications',
    },
  },
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireAuthOrRedirect(instance.id)
  },
})
