import { Link, useLocation, useMatches } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  CommandLineIcon,
  CpuIcon,
  DashboardSquare01Icon,
  Folder02Icon,
  Link04Icon,
  Notification03Icon,
  Settings01Icon,
  UserMultiple02Icon,
} from '@hugeicons/core-free-icons'
import {
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarMenu,
  SidebarMenuBadge,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarMenuSub,
  SidebarMenuSubButton,
  SidebarMenuSubItem,
} from '@/components/ui/sidebar'
import { Separator } from '@/components/ui/separator'
import { useAuthStore } from '@/stores/auth-store'
import { useRecentProjectsStore } from '@/stores/recent-projects-store'
import { useBuilds } from '@/hooks/use-builds'

interface NavItem {
  title: string
  to: string
  icon: typeof DashboardSquare01Icon
  adminOnly?: boolean
}

const PRIMARY_ITEMS: Array<NavItem> = [
  { title: 'Dashboard', to: '/', icon: DashboardSquare01Icon },
  { title: 'Projects', to: '/projects', icon: Folder02Icon },
  { title: 'Builds', to: '/builds', icon: CommandLineIcon },
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
    icon: CpuIcon,
    adminOnly: true,
  },
  {
    title: 'Sources',
    to: '/settings/integrations',
    icon: Link04Icon,
    adminOnly: true,
  },
  {
    title: 'Notifications',
    to: '/settings/notifications',
    icon: Notification03Icon,
    adminOnly: true,
  },
  {
    title: 'Preferences',
    to: '/settings/preferences',
    icon: Settings01Icon,
    adminOnly: true,
  },
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
  const isAdmin = authUser?.role === 'owner' || authUser?.role === 'admin'
  const recentProjects = useRecentProjectsStore((s) => s.projects)

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
                {item.to === '/builds' && <ActiveBuildBadge />}
                {item.to === '/projects' && recentProjects.length > 0 && (
                  <SidebarMenuSub>
                    {recentProjects.map((project) => (
                      <SidebarMenuSubItem key={project.id}>
                        <SidebarMenuSubButton
                          isActive={
                            location.pathname === `/projects/${project.id}`
                          }
                          render={
                            <Link
                              to="/projects/$projectId"
                              params={{ projectId: project.id }}
                            />
                          }
                        >
                          <span>{project.name}</span>
                        </SidebarMenuSubButton>
                      </SidebarMenuSubItem>
                    ))}
                  </SidebarMenuSub>
                )}
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
