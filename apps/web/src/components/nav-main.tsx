import { Link, useLocation, useMatches } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  CommandLineIcon,
  DashboardSquare01Icon,
  Folder02Icon,
  Settings01Icon,
} from '@hugeicons/core-free-icons'
import {
  SidebarGroup,
  SidebarGroupContent,
  SidebarMenu,
  SidebarMenuBadge,
  SidebarMenuButton,
  SidebarMenuItem,
} from '@/components/ui/sidebar'
import { useAuthStore } from '@/stores/auth-store'
import { useBuilds } from '@/hooks/use-builds'
import { canAccessSettings } from '@/components/settings/settings-navigation'

interface NavItem {
  title: string
  to: string
  icon: typeof DashboardSquare01Icon
}

const PRIMARY_ITEMS: Array<NavItem> = [
  { title: 'Dashboard', to: '/', icon: DashboardSquare01Icon },
  { title: 'Projects', to: '/projects', icon: Folder02Icon },
  { title: 'Builds', to: '/builds', icon: CommandLineIcon },
  { title: 'Settings', to: '/settings', icon: Settings01Icon },
]

function ActiveBuildBadge() {
  const { data } = useBuilds({ status: 'running', limit: 100 })
  const count = data?.builds.length ?? 0
  if (count === 0) return null
  return <SidebarMenuBadge>{count}</SidebarMenuBadge>
}

export default function NavMain() {
  const matches = useMatches()
  const location = useLocation()
  const authUser = useAuthStore((s) => s.user)
  const isQaViewer = authUser?.role === 'qa_viewer'

  function isActive(item: NavItem) {
    return item.to === '/'
      ? location.pathname === '/'
      : matches.some((m) => m.fullPath.startsWith(item.to))
  }

  const visiblePrimaryItems = PRIMARY_ITEMS.filter((item) => {
    if (isQaViewer) return item.to === '/'
    if (item.to === '/settings') return canAccessSettings(authUser?.role)
    return true
  })

  return (
    <>
      <SidebarGroup className="py-2">
        <SidebarGroupContent>
          <SidebarMenu>
            {visiblePrimaryItems.map((item) => (
              <SidebarMenuItem key={item.to}>
                <SidebarMenuButton
                  isActive={isActive(item)}
                  tooltip={item.title}
                  render={<Link to={item.to} />}
                >
                  <HugeiconsIcon icon={item.icon} size={18} />
                  <span>{item.title}</span>
                </SidebarMenuButton>
                {item.to === '/builds' && <ActiveBuildBadge />}
              </SidebarMenuItem>
            ))}
          </SidebarMenu>
        </SidebarGroupContent>
      </SidebarGroup>
    </>
  )
}
