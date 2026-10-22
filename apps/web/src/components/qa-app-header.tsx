import { lazy, Suspense, useMemo, useState } from 'react'
import { Link, useLocation, useParams } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import { Logout03Icon, Moon02Icon, Sun03Icon } from '@hugeicons/core-free-icons'
import { useTheme } from 'next-themes'

import { Avatar, AvatarFallback, AvatarImage } from '@/components/ui/avatar'
import {
  Breadcrumb,
  BreadcrumbItem,
  BreadcrumbLink,
  BreadcrumbList,
} from '@/components/ui/breadcrumb'
import { Button } from '@/components/ui/button'
import { Skeleton } from '@/components/ui/skeleton'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { useLogout } from '@/hooks/use-auth'
import { useBuild } from '@/hooks/use-builds'
import { useInfiniteProjects, useProject } from '@/hooks/use-projects'
import { useDebouncedCallback } from '@/hooks/use-debounced-callback'
import { useQaProjectSelection } from '@/hooks/use-qa-project'
import { PAGE_WIDTH_CLASSES } from '@/components/page-layout'
import QaAppMonogram from '@/components/qa-app-monogram'
import { cn } from '@/lib/utils'
import { useAuthStore } from '@/stores/auth-store'

const QaProjectPicker = lazy(() => import('./qa-project-picker'))

function initials(email: string) {
  const name = email.split('@')[0]
  return (
    name
      .split(/[._-]/)
      .slice(0, 2)
      .map((part) => part.at(0)?.toUpperCase())
      .join('') || 'QA'
  )
}

export default function QaAppHeader() {
  const location = useLocation()
  const { buildId = '' } = useParams({ strict: false })
  const user = useAuthStore((state) => state.user)
  const { project: selectedProject, setSelectedProjectId } =
    useQaProjectSelection()
  const logoutMutation = useLogout()
  const { resolvedTheme, setTheme } = useTheme()
  const isDark = resolvedTheme === 'dark'
  const ThemeIcon = isDark ? Sun03Icon : Moon02Icon
  const isReleasesHome = location.pathname === '/'
  const [projectSearch, setProjectSearch] = useState('')
  const updateProjectSearch = useDebouncedCallback(
    (value: string) => setProjectSearch(value.trim()),
    300,
  )
  const projectsQuery = useInfiniteProjects(
    {
      limit: 100,
      search: projectSearch || undefined,
      sort: 'name',
      direction: 'asc',
    },
    { enabled: isReleasesHome },
  )
  const projects = useMemo(
    () => projectsQuery.data?.pages.flatMap((page) => page.projects) ?? [],
    [projectsQuery.data?.pages],
  )
  const buildQuery = useBuild(isReleasesHome ? '' : buildId, {
    refetchInterval: false,
  })
  const detailProjectQuery = useProject(buildQuery.data?.build.project_id ?? '')
  const detailBuild = buildQuery.data?.build
  const detailProjectName =
    detailProjectQuery.data?.project.name ?? detailBuild?.context?.project_name
  const [pickerOpen, setPickerOpen] = useState(false)

  return (
    <header className="sticky top-0 z-30 border-b bg-background/95 pt-(--safe-area-top) backdrop-blur supports-[backdrop-filter]:bg-background/80">
      <div
        className={cn(
          PAGE_WIDTH_CLASSES.default,
          'mx-auto flex h-14 w-full items-center gap-3 px-4 sm:px-6',
        )}
      >
        <Link to="/" className="flex shrink-0 items-center gap-3">
          <img src="/logo.svg" alt="Oore CI" className="size-7" />
          <span className="hidden font-semibold tracking-tight sm:inline">
            Oore
          </span>
        </Link>
        <span className="h-4 w-px shrink-0 bg-border" aria-hidden />
        {isReleasesHome && selectedProject ? (
          <Suspense fallback={<Skeleton className="h-8 w-32" />}>
            <QaProjectPicker
              hasMoreProjects={Boolean(projectsQuery.hasNextPage)}
              isFetchingMoreProjects={projectsQuery.isFetchingNextPage}
              onLoadMoreProjects={() => void projectsQuery.fetchNextPage()}
              onOpenChange={setPickerOpen}
              onProjectChange={setSelectedProjectId}
              onSearchProjects={updateProjectSearch}
              open={pickerOpen}
              project={selectedProject}
              projects={projects}
            />
          </Suspense>
        ) : detailBuild && detailProjectName ? (
          <Breadcrumb className="min-w-0">
            <BreadcrumbList className="flex-nowrap">
              <BreadcrumbItem className="min-w-0">
                <BreadcrumbLink
                  render={
                    <Link
                      to="/"
                      onClick={() =>
                        setSelectedProjectId(detailBuild.project_id)
                      }
                    />
                  }
                  className="flex min-h-11 min-w-0 items-center gap-2"
                >
                  <QaAppMonogram name={detailProjectName} />
                  <span className="truncate">{detailProjectName}</span>
                </BreadcrumbLink>
              </BreadcrumbItem>
            </BreadcrumbList>
          </Breadcrumb>
        ) : (
          <span className="truncate text-sm text-muted-foreground">
            Test releases
          </span>
        )}

        <DropdownMenu>
          <DropdownMenuTrigger
            render={
              <Button
                variant="ghost"
                size="icon"
                className="ml-auto"
                aria-label="Open account menu"
              />
            }
          >
            <Avatar className="size-6">
              {user?.avatar_url ? (
                <AvatarImage src={user.avatar_url} alt={user.email} />
              ) : null}
              <AvatarFallback className="text-[0.625rem]">
                {user ? initials(user.email) : 'QA'}
              </AvatarFallback>
            </Avatar>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end" className="min-w-56">
            <DropdownMenuGroup>
              <DropdownMenuLabel className="p-0 font-normal">
                <span className="flex items-center gap-2 px-1 py-1.5">
                  <Avatar>
                    {user?.avatar_url ? (
                      <AvatarImage src={user.avatar_url} alt={user.email} />
                    ) : null}
                    <AvatarFallback>
                      {user ? initials(user.email) : 'QA'}
                    </AvatarFallback>
                  </Avatar>
                  <span className="grid min-w-0 flex-1 text-left leading-tight">
                    <span className="truncate text-sm font-medium text-foreground">
                      {user?.email}
                    </span>
                    <span className="truncate text-xs">QA viewer</span>
                  </span>
                </span>
              </DropdownMenuLabel>
            </DropdownMenuGroup>
            <DropdownMenuSeparator />
            <DropdownMenuItem
              onClick={() => setTheme(isDark ? 'light' : 'dark')}
            >
              <HugeiconsIcon icon={ThemeIcon} />
              {isDark ? 'Light mode' : 'Dark mode'}
            </DropdownMenuItem>
            <DropdownMenuItem
              onClick={() => logoutMutation.mutate()}
              disabled={logoutMutation.isPending}
            >
              <HugeiconsIcon icon={Logout03Icon} />
              Sign out
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
    </header>
  )
}
