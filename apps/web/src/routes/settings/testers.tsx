import { createFileRoute } from '@tanstack/react-router'

import {
  getActiveInstanceOrRedirect,
  requireInstanceRoleOrRedirect,
} from '@/lib/instance-context'

export const Route = createFileRoute('/settings/testers')({
  staticData: {
    breadcrumb: {
      title: 'Tester groups',
    },
  },
  beforeLoad: (context) => {
    const instance = getActiveInstanceOrRedirect()
    requireInstanceRoleOrRedirect(instance.id, ['owner', 'admin'], context)
  },
})
