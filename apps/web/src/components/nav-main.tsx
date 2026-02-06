import { Link, useLocation, useMatches } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import { DashboardSquare01Icon, UserMultiple02Icon } from '@hugeicons/core-free-icons'
import {
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
} from '@/components/ui/sidebar'
import { useAuthStore } from '@/stores/auth-store'

interface NavItem {
  title: string
  to: string
  icon: typeof DashboardSquare01Icon
  adminOnly?: boolean
}

interface NavGroup {
  label: string
  items: Array<NavItem>
}

const NAV_GROUPS: Array<NavGroup> = [
  {
    label: 'Platform',
    items: [
      { title: 'Dashboard', to: '/', icon: DashboardSquare01Icon },
    ],
  },
  {
    label: 'Management',
    items: [
      {
        title: 'Users',
        to: '/settings/users',
        icon: UserMultiple02Icon,
        adminOnly: true,
      },
    ],
  },
]

export default function NavMain() {
  const matches = useMatches()
  const location = useLocation()
  const authUser = useAuthStore((s) => s.user)
  const isAdmin = authUser?.role === 'owner' || authUser?.role === 'admin'

  return (
    <>
      {NAV_GROUPS.map((group) => {
        const visibleItems = group.items.filter(
          (item) => !item.adminOnly || isAdmin,
        )
        if (visibleItems.length === 0) return null

        return (
          <SidebarGroup key={group.label}>
            <SidebarGroupLabel>{group.label}</SidebarGroupLabel>
            <SidebarGroupContent>
              <SidebarMenu>
                {visibleItems.map((item) => {
                  const isActive =
                    item.to === '/'
                      ? location.pathname === '/'
                      : matches.some((m) =>
                          m.fullPath.startsWith(item.to),
                        )

                  return (
                    <SidebarMenuItem key={item.to}>
                      <SidebarMenuButton
                        isActive={isActive}
                        render={<Link to={item.to} />}
                      >
                        <HugeiconsIcon icon={item.icon} size={18} />
                        <span>{item.title}</span>
                      </SidebarMenuButton>
                    </SidebarMenuItem>
                  )
                })}
              </SidebarMenu>
            </SidebarGroupContent>
          </SidebarGroup>
        )
      })}
    </>
  )
}
