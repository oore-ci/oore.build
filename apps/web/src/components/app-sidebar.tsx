import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarHeader,
  SidebarRail,
} from '@/components/ui/sidebar'
import InstanceSwitcher from '@/components/instance-switcher'
import NavMain from '@/components/nav-main'
import NavUser from '@/components/nav-user'
import RuntimeUpdateNotice from '@/components/runtime-update-notice'
import { useActiveInstance } from '@/stores/instance-store'
import { useAuthStore } from '@/stores/auth-store'

export default function AppSidebar(
  props: React.ComponentProps<typeof Sidebar>,
) {
  const instance = useActiveInstance()
  const token = useAuthStore((state) => state.token)
  const user = useAuthStore((state) => state.user)

  if (!instance || !token || !user) return null

  return (
    <Sidebar collapsible="offcanvas" {...props}>
      <SidebarHeader>
        <InstanceSwitcher />
      </SidebarHeader>
      <SidebarContent>
        <NavMain />
      </SidebarContent>
      <SidebarFooter>
        {user.role === 'owner' && <RuntimeUpdateNotice />}
        <NavUser />
      </SidebarFooter>
      <SidebarRail />
    </Sidebar>
  )
}
