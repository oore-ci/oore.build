import { createFileRoute, redirect } from '@tanstack/react-router'

import { settingsGroupsForRole } from '@/components/settings/settings-navigation'
import { useAuthStore } from '@/stores/auth-store'

/**
 * `/settings` has no page of its own: the sidebar lists every settings page,
 * so this sends the user to the first one their role can open.
 */
export const Route = createFileRoute('/settings/')({
  beforeLoad: () => {
    const role = useAuthStore.getState().user?.role
    const first = settingsGroupsForRole(role)[0]?.items[0]
    throw redirect({ to: first?.to ?? '/', replace: true })
  },
})
