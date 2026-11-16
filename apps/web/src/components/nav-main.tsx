import { useState } from 'react'
import { Link, useLocation } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  Analytics01Icon,
  ArrowDown01Icon,
  CommandLineIcon,
  DashboardSquare02Icon,
  Folder02Icon,
} from '@hugeicons/core-free-icons'
import {
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarMenu,
  SidebarMenuBadge,
  SidebarMenuButton,
  SidebarMenuItem,
} from '@/components/ui/sidebar'
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible'
import { useAuthStore } from '@/stores/auth-store'
import { cn } from '@/lib/utils'
import { useSidebarSettingsOpen } from '@/stores/sidebar-settings-store'
import { useBuilds } from '@/hooks/use-builds'
import { settingsGroupsForRole } from '@/components/settings/settings-navigation'
import type { UserRole } from '@oore/client/models'

interface NavItem {
  title: string
  to: string
  icon: typeof DashboardSquare02Icon
}

interface NavGroup {
  title: string
  items: Array<NavItem>
  /** Small labelled runs of items inside the group, in order. */
  sections?: Array<{ title: string | null; items: Array<NavItem> }>
  /** The group's heading toggles it open and closed in place. */
  collapsible?: boolean
}

export const WORKSPACE_ITEMS: Array<NavItem> = [
  { title: 'Home', to: '/', icon: DashboardSquare02Icon },
  { title: 'Apps', to: '/projects', icon: Folder02Icon },
  { title: 'Builds', to: '/builds', icon: CommandLineIcon },
  { title: 'Insights', to: '/insights', icon: Analytics01Icon },
]

/**
 * Settings pages are listed directly so every one is a single click away;
 * there is no hub page in between. The list is role-filtered by the same
 * table the command palette uses.
 */
export function sidebarGroupsForRole(
  role: UserRole | undefined,
): Array<NavGroup> {
  if (!role || role === 'qa_viewer') return []

  const sections = settingsGroupsForRole(role).map((group) => ({
    title: group.title,
    items: group.items.map((item) => ({
      title: item.title,
      to: item.to,
      icon: item.icon,
    })),
  }))

  return [
    { title: 'Workspace', items: WORKSPACE_ITEMS },
    {
      title: 'Settings',
      items: sections.flatMap((section) => section.items),
      sections,
      collapsible: true,
    },
  ]
}

export function isSidebarItemActive(pathname: string, to: string): boolean {
  const normalizedPathname =
    pathname.length > 1 ? pathname.replace(/\/+$/, '') : pathname

  if (to === '/') return normalizedPathname === '/'

  return normalizedPathname === to || normalizedPathname.startsWith(`${to}/`)
}

/** Builds a runner holds now, counted the way Home's Waiting now card does. */
function ActiveBuildBadge() {
  const { data } = useBuilds({ status: 'assigned,running', limit: 1 })
  const count = data?.total ?? 0
  if (count === 0) return null
  return (
    <SidebarMenuBadge aria-label={`${count} running`}>{count}</SidebarMenuBadge>
  )
}

/**
 * Whether the Settings group shows its pages. It opens on every settings page
 * so the current page is visible, unless the person folded it on that page;
 * elsewhere it follows the person's last choice, and starts open only for
 * people who can change instance settings.
 */
export function isSettingsGroupOpen({
  remembered,
  role,
  onSettingsPage,
  foldedHere = false,
}: {
  remembered: boolean | undefined
  role: UserRole | undefined
  onSettingsPage: boolean
  foldedHere?: boolean
}): boolean {
  if (onSettingsPage) return !foldedHere
  if (remembered !== undefined) return remembered
  return role === 'owner' || role === 'admin'
}

function NavGroupItems({
  items,
  pathname,
}: {
  items: Array<NavItem>
  pathname: string
}) {
  return (
    <SidebarMenu>
      {items.map((item) => (
        <SidebarMenuItem key={item.to}>
          <SidebarMenuButton
            isActive={isSidebarItemActive(pathname, item.to)}
            tooltip={item.title}
            render={<Link to={item.to} />}
          >
            <HugeiconsIcon icon={item.icon} />
            <span>{item.title}</span>
          </SidebarMenuButton>
          {item.to === '/builds' && <ActiveBuildBadge />}
        </SidebarMenuItem>
      ))}
    </SidebarMenu>
  )
}

function CollapsibleNavGroup({
  group,
  pathname,
  role,
}: {
  group: NavGroup
  pathname: string
  role: UserRole | undefined
}) {
  const [remembered, setRemembered] = useSidebarSettingsOpen()
  const onSettingsPage = group.items.some((item) =>
    isSidebarItemActive(pathname, item.to),
  )
  // The settings page the person folded the group on, if any. Moving to
  // another settings page opens it again so the current page stays visible.
  const [foldedOn, setFoldedOn] = useState<string | null>(null)
  const open = isSettingsGroupOpen({
    remembered,
    role,
    onSettingsPage,
    foldedHere: foldedOn === pathname,
  })

  return (
    <Collapsible
      open={open}
      onOpenChange={(next) => {
        setRemembered(next)
        setFoldedOn(!next && onSettingsPage ? pathname : null)
      }}
      render={<SidebarGroup />}
    >
      <SidebarGroupLabel
        render={
          <CollapsibleTrigger className="w-full justify-between hover:bg-sidebar-accent hover:text-sidebar-accent-foreground" />
        }
      >
        {group.title}
        <HugeiconsIcon
          icon={ArrowDown01Icon}
          className={cn(
            'transition-transform motion-reduce:transition-none',
            !open && '-rotate-90',
          )}
        />
      </SidebarGroupLabel>
      <CollapsibleContent>
        <SidebarGroupContent>
          {(group.sections ?? [{ title: null, items: group.items }]).map(
            (section) => (
              <div key={section.title ?? 'top'}>
                {section.title ? (
                  <p className="px-2 pt-3 pb-1 text-[11px] text-sidebar-foreground/60 group-data-[collapsible=icon]:hidden">
                    {section.title}
                  </p>
                ) : null}
                <NavGroupItems items={section.items} pathname={pathname} />
              </div>
            ),
          )}
        </SidebarGroupContent>
      </CollapsibleContent>
    </Collapsible>
  )
}

export default function NavMain() {
  const location = useLocation()
  const authUser = useAuthStore((s) => s.user)
  const groups = sidebarGroupsForRole(authUser?.role)

  return (
    <>
      {groups.map((group) =>
        group.collapsible ? (
          <CollapsibleNavGroup
            key={group.title}
            group={group}
            pathname={location.pathname}
            role={authUser?.role}
          />
        ) : (
          <SidebarGroup key={group.title}>
            <SidebarGroupLabel>{group.title}</SidebarGroupLabel>
            <SidebarGroupContent>
              <NavGroupItems items={group.items} pathname={location.pathname} />
            </SidebarGroupContent>
          </SidebarGroup>
        ),
      )}
    </>
  )
}
