import { Link, useLocation } from '@tanstack/react-router'
import {
  Command as CommandLineIcon,
  LayoutDashboard as DashboardSquare01Icon,
  Folder as Folder02Icon,
} from 'lucide-react'
import {
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarMenu,
  SidebarMenuBadge,
  SidebarMenuButton,
  SidebarMenuItem,
} from '@/components/ui/sidebar'
import { useAuthStore } from '@/stores/auth-store'
import { useBuilds } from '@/hooks/use-builds'
import { settingsGroupsForRole } from '@/components/settings/settings-navigation'
import type { UserRole } from '@/lib/types'

interface NavItem {
  title: string
  to: string
  icon: typeof DashboardSquare01Icon
}

interface NavGroup {
  title: string
  items: Array<NavItem>
}

const WORKSPACE_ITEMS: Array<NavItem> = [
  { title: 'Dashboard', to: '/', icon: DashboardSquare01Icon },
  { title: 'Projects', to: '/projects', icon: Folder02Icon },
  { title: 'Builds', to: '/builds', icon: CommandLineIcon },
]

export function sidebarGroupsForRole(
  role: UserRole | undefined,
): Array<NavGroup> {
  if (!role || role === 'qa_viewer') return []

  const settingsItems = settingsGroupsForRole(role).flatMap(
    (group) => group.items,
  )

  return [
    { title: 'Workspace', items: WORKSPACE_ITEMS },
    {
      title: 'Settings',
      items: settingsItems,
    },
  ]
}

export function isSidebarItemActive(pathname: string, to: string): boolean {
  const normalizedPathname =
    pathname.length > 1 ? pathname.replace(/\/+$/, '') : pathname

  if (to === '/') return normalizedPathname === '/'

  return normalizedPathname === to || normalizedPathname.startsWith(`${to}/`)
}

function ActiveBuildBadge() {
  const { data } = useBuilds({ status: 'running', limit: 100 })
  const count = data?.builds.length ?? 0
  if (count === 0) return null
  return <SidebarMenuBadge>{count}</SidebarMenuBadge>
}

export default function NavMain() {
  const location = useLocation()
  const authUser = useAuthStore((s) => s.user)
  const groups = sidebarGroupsForRole(authUser?.role)

  return (
    <>
      {groups.map((group) => (
        <SidebarGroup key={group.title}>
          <SidebarGroupLabel>{group.title}</SidebarGroupLabel>
          <SidebarGroupContent>
            <SidebarMenu>
              {group.items.map((item) => {
                const Icon = item.icon

                return (
                  <SidebarMenuItem key={item.to}>
                    <SidebarMenuButton
                      isActive={isSidebarItemActive(location.pathname, item.to)}
                      tooltip={item.title}
                      render={<Link to={item.to} />}
                    >
                      <Icon />
                      <span>{item.title}</span>
                    </SidebarMenuButton>
                    {item.to === '/builds' && <ActiveBuildBadge />}
                  </SidebarMenuItem>
                )
              })}
            </SidebarMenu>
          </SidebarGroupContent>
        </SidebarGroup>
      ))}
    </>
  )
}
