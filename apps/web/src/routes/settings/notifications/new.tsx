import { createFileRoute } from '@tanstack/react-router'

import {
  getActiveInstanceOrRedirect,
  requireInstanceRoleOrRedirect,
} from '@/lib/instance-context'

export const Route = createFileRoute('/settings/notifications/new')({
  staticData: {
    breadcrumbLabel: 'New channel',
    breadcrumbParent: {
      label: 'Notifications',
      to: '/settings/notifications',
    },
  },
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireInstanceRoleOrRedirect(instance.id, ['owner', 'admin'])
  },
})
