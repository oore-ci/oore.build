import { createFileRoute, redirect } from '@tanstack/react-router'

import {
  getActiveInstanceOrRedirect,
  requireAuthOrRedirect,
} from '@/lib/instance-context'
import { useAuthStore } from '@/stores/auth-store'

export const Route = createFileRoute('/settings/retention')({
  staticData: { breadcrumb: {
   title: 'Retention' ,
 },},
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireAuthOrRedirect(instance.id)

    const user = useAuthStore.getState().user
    if (!user || (user.role !== 'owner' && user.role !== 'admin')) {
      throw redirect({ to: '/' })
    }
  },
})
