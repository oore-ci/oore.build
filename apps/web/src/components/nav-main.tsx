import { Link, useLocation, useMatches } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  DashboardSquare01Icon,
  Folder02Icon,
  GitBranchIcon,
  Link04Icon,
  UserMultiple02Icon,
} from '@hugeicons/core-free-icons'
import {
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
} from '@/components/ui/sidebar'
import { Separator } from '@/components/ui/separator'
import { useAuthStore } from '@/stores/auth-store'

interface NavItem {
  title: string
  to: string
  icon: typeof DashboardSquare01Icon
  adminOnly?: boolean
}

const PRIMARY_ITEMS: Array<NavItem> = [
  { title: 'Dashboard', to: '/', icon: DashboardSquare01Icon },
  { title: 'Projects', to: '/projects', icon: Folder02Icon },
  { title: 'Builds', to: '/builds', icon: GitBranchIcon },
]

const ADMIN_ITEMS: Array<NavItem> = [
  {
    title: 'Users',
    to: '/settings/users',
    icon: UserMultiple02Icon,
    adminOnly: true,
  },
  {
    title: 'Runners',
    to: '/settings/runners',
    icon: GitBranchIcon,
    adminOnly: true,
  },
  {
    title: 'Integrations',
    to: '/settings/integrations',
    icon: Link04Icon,
    adminOnly: true,
  },
  {
    title: 'Preferences',
    to: '/settings/preferences',
    icon: Folder02Icon,
    adminOnly: true,
  },
]

export default function NavMain() {
  const matches = useMatches()
  const location = useLocation()
  const authUser = useAuthStore((s) => s.user)
  const isAdmin = authUser?.role === 'owner' || authUser?.role === 'admin'

  function isActive(item: NavItem) {
    return item.to === '/'
      ? location.pathname === '/'
      : matches.some((m) => m.fullPath.startsWith(item.to))
  }

  const visibleAdminItems = ADMIN_ITEMS.filter(
    (item) => !item.adminOnly || isAdmin,
  )

  return (
    <>
      <SidebarGroup>
        <SidebarGroupLabel>Operations</SidebarGroupLabel>
        <SidebarGroupContent>
          <SidebarMenu>
            {PRIMARY_ITEMS.map((item) => (
              <SidebarMenuItem key={item.to}>
                <SidebarMenuButton
                  isActive={isActive(item)}
                  render={<Link to={item.to} />}
                >
                  <HugeiconsIcon icon={item.icon} size={18} />
                  <span>{item.title}</span>
                </SidebarMenuButton>
              </SidebarMenuItem>
            ))}
          </SidebarMenu>
        </SidebarGroupContent>
      </SidebarGroup>

      {visibleAdminItems.length > 0 && (
        <>
          <Separator className="mx-2" />
          <SidebarGroup>
            <SidebarGroupLabel>Admin</SidebarGroupLabel>
            <SidebarGroupContent>
              <SidebarMenu>
                {visibleAdminItems.map((item) => (
                  <SidebarMenuItem key={item.to}>
                    <SidebarMenuButton
                      isActive={isActive(item)}
                      render={<Link to={item.to} />}
                    >
                      <HugeiconsIcon icon={item.icon} size={18} />
                      <span>{item.title}</span>
                    </SidebarMenuButton>
                  </SidebarMenuItem>
                ))}
              </SidebarMenu>
            </SidebarGroupContent>
          </SidebarGroup>
        </>
      )}
    </>
  )
}
