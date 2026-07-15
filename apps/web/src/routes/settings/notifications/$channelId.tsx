import { createFileRoute } from '@tanstack/react-router'

import {
  getActiveInstanceOrRedirect,
  requireInstanceRoleOrRedirect,
} from '@/lib/instance-context'

export const Route = createFileRoute('/settings/notifications/$channelId')({
  staticData: {
    breadcrumbLabel: 'Channel',
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
