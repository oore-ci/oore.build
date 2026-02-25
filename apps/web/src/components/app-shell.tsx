import { For, Show, createMemo, type JSX } from 'solid-js'
import { Link, useLocation, useMatches } from '@tanstack/solid-router'
import {
  ArrowRight01Icon,
  ArrowUpDownIcon,
  DashboardSquare01Icon,
  Folder02Icon,
  GitBranchIcon,
  Link04Icon,
  Moon02Icon,
  SidebarLeftIcon,
  Sun03Icon,
  UserMultiple02Icon,
} from '@hugeicons/core-free-icons'
import { HugeIcon } from '@/components/huge-icon'
import { useBuilds } from '@/hooks/use-builds'
import { Button } from '@/components/ui/button'
import { Separator } from '@/components/ui/separator'
import { getInstanceIcon } from '@/lib/instance-icons'
import { useAuthStore } from '@/stores/auth-store'
import { useBreadcrumbStore } from '@/stores/breadcrumb-store'
import { useRecentProjectsStore } from '@/stores/recent-projects-store'
import { useInstanceStore } from '@/stores/instance-store'

interface NavItem {
  to: string
  label: string
  icon: typeof DashboardSquare01Icon
  adminOnly?: boolean
  showRunningCount?: boolean
}

const primaryNavItems: Array<NavItem> = [
  { to: '/', label: 'Dashboard', icon: DashboardSquare01Icon },
  { to: '/projects', label: 'Projects', icon: Folder02Icon },
  {
    to: '/builds',
    label: 'Builds',
    icon: GitBranchIcon,
    showRunningCount: true,
  },
] as const

const adminNavItems: Array<NavItem> = [
  { to: '/settings/users', label: 'Users', icon: UserMultiple02Icon, adminOnly: true },
  { to: '/settings/runners', label: 'Runners', icon: GitBranchIcon, adminOnly: true },
  {
    to: '/settings/integrations',
    label: 'Sources',
    icon: Link04Icon,
    adminOnly: true,
  },
  {
    to: '/settings/preferences',
    label: 'Preferences',
    icon: Folder02Icon,
    adminOnly: true,
  },
] as const

function isDarkModeEnabled(): boolean {
  return document.documentElement.classList.contains('dark')
}

function setDarkMode(enabled: boolean): void {
  document.documentElement.classList.toggle('dark', enabled)
  try {
    localStorage.setItem('oore_ui_theme', enabled ? 'dark' : 'light')
  } catch {
    // localStorage unavailable
  }
}

(function bootstrapTheme() {
  if (typeof document === 'undefined') return
  try {
    const saved = localStorage.getItem('oore_ui_theme')
    if (saved === 'dark') {
      setDarkMode(true)
    } else if (saved === 'light') {
      setDarkMode(false)
    }
  } catch {
    // localStorage unavailable
  }
})()

function initialsFromEmail(email: string): string {
  const [left] = email.split('@')
  return left
    .split(/[._-]/)
    .filter(Boolean)
    .slice(0, 2)
    .map((part) => part[0]?.toUpperCase() ?? '')
    .join('')
}

function humanize(segment: string): string {
  return segment.replace(/[-_]/g, ' ').replace(/\b\w/g, (char) => char.toUpperCase())
}

export function AppShell(props: { children: JSX.Element }) {
  const matches = useMatches()
  const location = useLocation()
  const breadcrumbLabels = useBreadcrumbStore((state) => state.labels)
  const activeInstanceId = useInstanceStore((state) => state.activeInstanceId)
  const instances = useInstanceStore((state) => state.instances)
  const authToken = useAuthStore((state) => state.token)
  const authUser = useAuthStore((state) => state.user)
  const recentProjects = useRecentProjectsStore((state) => state.projects)
  const runningBuilds = useBuilds({ status: 'running', limit: 100 })

  const instance = createMemo(() => {
    const id = activeInstanceId()
    if (!id) return null
    return instances()[id] ?? null
  })
  const instanceHost = createMemo(() => {
    const current = instance()
    if (!current) return 'local'
    try {
      return new URL(current.url).hostname
    } catch {
      return current.url || 'local'
    }
  })

  const runningBuildCount = () => runningBuilds.data?.builds.length ?? 0
  const isAdmin = () => {
    const role = authUser()?.role
    return role === 'owner' || role === 'admin'
  }

  const visibleAdminItems = () =>
    adminNavItems.filter((item) => !item.adminOnly || isAdmin())

  const showChrome = () => {
    const isSetupRoute = matches().some((match) =>
      match.routeId.startsWith('/setup'),
    )
    const isLoginRoute = matches().some(
      (match) => match.routeId === '/login' || match.routeId === '/auth/callback',
    )

    return (
      !isSetupRoute &&
      !isLoginRoute &&
      !!activeInstanceId() &&
      !!authToken() &&
      !!authUser()
    )
  }

  const activeNav = (to: string) => {
    const pathname = location().pathname
    if (to === '/') return pathname === '/'
    return pathname.startsWith(to)
  }

  const breadcrumbItems = () => {
    const labels = breadcrumbLabels()
    const current = matches()
    const crumbs: Array<string> = []

    const isSettingsRoute = current.some((match) =>
      (match as { fullPath?: string }).fullPath?.startsWith('/settings'),
    )

    for (const match of current) {
      const routeId = (match as { routeId?: string }).routeId
      if (!routeId || routeId === '__root__') continue

      const staticLabel = (
        match as { staticData?: { breadcrumbLabel?: string } }
      ).staticData?.breadcrumbLabel
      const dynamicLabel = labels[routeId]
      const label = dynamicLabel || staticLabel
      if (label) crumbs.push(label)
    }

    if (isSettingsRoute && !crumbs.includes('Settings')) {
      crumbs.unshift('Settings')
    }

    if (crumbs.length > 0) return crumbs
    if (location().pathname === '/') return ['Dashboard']

    const lastSegment =
      location().pathname.split('/').filter(Boolean).slice(-1)[0] ?? 'Dashboard'
    return [humanize(lastSegment)]
  }

  const navLinkClass = (active: boolean) =>
    active
      ? 'flex items-center justify-between rounded-none bg-sidebar-primary px-2 py-1.5 text-sm text-sidebar-primary-foreground'
      : 'flex items-center justify-between rounded-none px-2 py-1.5 text-sm text-sidebar-foreground/75 hover:bg-sidebar-accent hover:text-sidebar-accent-foreground'

  return (
    <Show
      when={showChrome()}
      fallback={<div class="min-h-screen bg-surface">{props.children}</div>}
    >
      <div class="min-h-screen bg-surface md:grid md:grid-cols-[220px_1fr]">
        <aside class="flex h-screen flex-col border-r bg-sidebar">
          <div class="border-b px-2 py-1.5">
            <div class="flex items-center gap-2 rounded-none px-1 py-1">
              <div class="bg-sidebar-primary text-sidebar-primary-foreground flex size-8 items-center justify-center">
                <HugeIcon icon={getInstanceIcon(instance()?.icon)} size={16} />
              </div>
              <div class="grid min-w-0 flex-1 text-left text-sm leading-tight">
                <span class="truncate font-medium">
                  {instance()?.label ?? 'No instance'}
                </span>
                <span class="truncate text-xs text-muted-foreground">
                  {instanceHost()}
                </span>
              </div>
              <Link
                to="/login"
                class="text-muted-foreground hover:text-foreground"
                aria-label="Switch instance"
                title="Switch instance"
              >
                <HugeIcon icon={ArrowUpDownIcon} size={16} />
              </Link>
            </div>
          </div>

          <nav class="flex-1 overflow-y-auto p-2">
            <p class="px-1 pb-2 text-xs font-medium uppercase tracking-wider text-muted-foreground">
              Operations
            </p>
            <div class="space-y-0.5">
              <For each={primaryNavItems}>
                {(item) => (
                  <Link to={item.to} class={navLinkClass(activeNav(item.to))}>
                    <span class="flex items-center gap-2">
                      <HugeIcon icon={item.icon} size={16} />
                      {item.label}
                    </span>
                    <Show when={item.showRunningCount && runningBuildCount() > 0}>
                      <span class="text-xs font-medium">
                        {runningBuildCount()}
                      </span>
                    </Show>
                  </Link>
                )}
              </For>
            </div>

            <Show when={visibleAdminItems().length > 0}>
              <div class="my-3 border-t" />
              <p class="px-1 pb-2 text-xs font-medium uppercase tracking-wider text-muted-foreground">
                Admin
              </p>
              <div class="space-y-0.5">
                <For each={visibleAdminItems()}>
                  {(item) => (
                    <Link to={item.to} class={navLinkClass(activeNav(item.to))}>
                      <span class="flex items-center gap-2">
                        <HugeIcon icon={item.icon} size={16} />
                        {item.label}
                      </span>
                    </Link>
                  )}
                </For>
              </div>
            </Show>

            <Show when={recentProjects().length > 0}>
              <div class="my-3 border-t" />
              <p class="px-1 pb-2 text-xs font-medium uppercase tracking-wider text-muted-foreground">
                Recent Projects
              </p>
              <div class="space-y-0.5">
                <For each={recentProjects()}>
                  {(project) => (
                    <Link
                      to="/projects/$projectId"
                      params={{ projectId: project.id }}
                      class={navLinkClass(
                        location().pathname === `/projects/${project.id}`,
                      )}
                    >
                      <span class="truncate text-xs">{project.name}</span>
                    </Link>
                  )}
                </For>
              </div>
            </Show>
          </nav>

          <div class="border-t p-2">
            <div class="flex items-center gap-2 rounded-none px-1 py-1.5">
              <div class="flex size-6 items-center justify-center rounded-full border bg-muted text-[10px] font-semibold">
                {initialsFromEmail(authUser()?.email ?? 'U')}
              </div>
              <div class="min-w-0">
                <p class="truncate text-xs font-medium">{authUser()?.email}</p>
                <p class="truncate text-xs text-muted-foreground capitalize">
                  {authUser()?.role}
                </p>
              </div>
            </div>
          </div>
        </aside>

        <main class="min-w-0 bg-surface">
          <header class="sticky top-0 z-30 flex h-12 items-center justify-between border-b bg-background px-4">
            <div class="flex items-center gap-2">
              <span class="-ml-1 inline-flex size-7 items-center justify-center text-muted-foreground">
                <HugeIcon icon={SidebarLeftIcon} size={18} />
              </span>
              <Separator orientation="vertical" class="mr-1 h-4 self-auto" />
              <Link to="/" class="flex items-center gap-2">
                <img src="/logo.svg" alt="Oore CI logo" class="size-5" />
                <span class="text-sm font-semibold tracking-tight">Oore CI</span>
              </Link>
              <Separator orientation="vertical" class="h-4" />
              <div class="flex items-center gap-1 text-sm text-muted-foreground">
                <For each={breadcrumbItems()}>
                  {(crumb, index) => (
                    <>
                      <Show when={index() > 0}>
                        <HugeIcon icon={ArrowRight01Icon} size={12} />
                      </Show>
                      <span>{crumb}</span>
                    </>
                  )}
                </For>
              </div>
            </div>
            <div class="flex items-center gap-2">
              <Button
                variant="ghost"
                size="icon"
                onClick={() => setDarkMode(!isDarkModeEnabled())}
                aria-label="Toggle theme"
              >
                <Show when={isDarkModeEnabled()} fallback={<HugeIcon icon={Moon02Icon} />}>
                  <HugeIcon icon={Sun03Icon} />
                </Show>
              </Button>
              <Separator orientation="vertical" class="h-5" />
              <Link to="/login" class="text-xs text-muted-foreground hover:text-foreground">
                Switch instance
              </Link>
            </div>
          </header>
          <div class="flex flex-1 flex-col bg-surface">{props.children}</div>
        </main>
      </div>
    </Show>
  )
}
