import { createFileRoute } from '@tanstack/react-router'

import {
  getActiveInstanceOrRedirect,
  requireInstanceRoleOrRedirect,
} from '@/lib/instance-context'

export const Route = createFileRoute('/settings/theme')({
  staticData: {
    breadcrumb: {
      title: 'Theme',
    },
  },
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireInstanceRoleOrRedirect(instance.id, ['owner', 'admin', 'developer'])
  },
})
