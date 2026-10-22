import {
  getActiveInstanceOrRedirect,
  requireInstanceRoleOrRedirect,
} from '@/lib/instance-context'
import { createFileRoute } from '@tanstack/react-router'

export const Route = createFileRoute('/settings')({
  staticData: {
    breadcrumb: {
      title: 'Settings',
      link: false,
    },
  },
  beforeLoad: (context) => {
    const instance = getActiveInstanceOrRedirect()
    requireInstanceRoleOrRedirect(
      instance.id,
      ['owner', 'admin', 'developer'],
      context,
    )
  },
})
