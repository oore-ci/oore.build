import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarHeader,
  SidebarRail,
} from '@/components/ui/sidebar'
import InstanceSwitcher from '@/components/InstanceSwitcher'
import NavMain from '@/components/nav-main'
import NavUser from '@/components/nav-user'
import { useActiveInstance } from '@/stores/instance-store'
import { useAuthStore } from '@/stores/auth-store'

export default function AppSidebar(
  props: React.ComponentProps<typeof Sidebar>,
) {
  const instance = useActiveInstance()
  const authToken = useAuthStore((s) => s.token)
  const authUser = useAuthStore((s) => s.user)

  if (!instance || !authToken || !authUser) return null

  return (
    <Sidebar collapsible="icon" {...props}>
      <SidebarHeader>
        <InstanceSwitcher />
      </SidebarHeader>
      <SidebarContent>
        <NavMain />
      </SidebarContent>
      <SidebarFooter>
        <NavUser />
      </SidebarFooter>
      <SidebarRail />
    </Sidebar>
  )
}
