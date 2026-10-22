import { createFileRoute } from '@tanstack/react-router'

import {
  getActiveInstanceOrRedirect,
  requireInstanceRoleOrRedirect,
} from '@/lib/instance-context'

export const Route = createFileRoute('/settings/secrets')({
  staticData: {
    breadcrumb: {
      title: 'Secrets',
    },
  },
  beforeLoad: (context) => {
    const instance = getActiveInstanceOrRedirect()
    requireInstanceRoleOrRedirect(instance.id, ['owner', 'admin'], context)
  },
})
