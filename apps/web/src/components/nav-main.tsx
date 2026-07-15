import { Link, useLocation, useMatches } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  Audit01Icon,
  CommandLineIcon,
  CpuIcon,
  DashboardSquare01Icon,
  Delete02Icon,
  Folder02Icon,
  Key01Icon,
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
    title: 'API Tokens',
    to: '/settings/api-tokens',
    icon: Key01Icon,
    adminOnly: false,
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
    title: 'Retention',
    to: '/settings/retention',
    icon: Delete02Icon,
    adminOnly: true,
  },
  {
    title: 'Preferences',
    to: '/settings/preferences',
    icon: Settings01Icon,
    adminOnly: true,
  },
  {
    title: 'Audit Log',
    to: '/settings/audit-log',
    icon: Audit01Icon,
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
  const isQaViewer = authUser?.role === 'qa_viewer'
  const isDeveloperOrAbove = isAdmin || authUser?.role === 'developer'
  const recentProjects = useRecentProjectsStore((s) => s.projects)

  function isActive(item: NavItem) {
    return item.to === '/'
      ? location.pathname === '/'
      : matches.some((m) => m.fullPath.startsWith(item.to))
  }

  const visibleAdminItems = ADMIN_ITEMS.filter((item) => {
    if (
      item.to === '/settings/api-tokens' ||
      item.to === '/settings/runners' ||
      item.to === '/settings/integrations'
    ) {
      return isDeveloperOrAbove
    }
    return !item.adminOnly || isAdmin
  })
  const visiblePrimaryItems = isQaViewer
    ? PRIMARY_ITEMS.filter((item) => item.to === '/')
    : PRIMARY_ITEMS

  return (
    <>
      <SidebarGroup>
        <SidebarGroupLabel>Operations</SidebarGroupLabel>
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
            <SidebarGroupLabel>Settings</SidebarGroupLabel>
            <SidebarGroupContent>
              <SidebarMenu>
                {visibleAdminItems.map((item) => (
                  <SidebarMenuItem key={item.to}>
                    <SidebarMenuButton
                      isActive={isActive(item)}
                      tooltip={item.title}
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
