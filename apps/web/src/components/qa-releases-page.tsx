import { useMemo, useState } from 'react'
import { Link } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  ArrowRight01Icon,
  RefreshIcon,
  Search01Icon,
  SmartPhone01Icon,
} from '@hugeicons/core-free-icons'

import type { Artifact, Build, Project } from '@/lib/types'
import { useArtifactsForBuilds, useBuilds } from '@/hooks/use-builds'
import { useProjectPages } from '@/hooks/use-projects'
import { useDebouncedCallback } from '@/hooks/use-debounced-callback'
import { usePageClamp } from '@/hooks/use-page-clamp'
import {
  detectInstallDevice,
  selectInstallArtifact,
} from '@/lib/artifact-install'
import { relativeTime } from '@/lib/format-utils'
import {
  changelogSummary,
  qaBuildVersion,
  qaProjectVersionBase,
} from '@/lib/qa-releases'
import { PageMeta } from '@/lib/seo'
import { getStatusVariant } from '@/lib/status-variants'
import PageLayout from '@/components/page-layout'
import RepositoryAvatar from '@/components/repository-avatar'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from '@/components/ui/empty'
import { Input } from '@/components/ui/input'
import {
  Pagination,
  PaginationContent,
  PaginationEllipsis,
  PaginationItem,
  PaginationLink,
  PaginationNext,
  PaginationPrevious,
} from '@/components/ui/pagination'
import { Skeleton } from '@/components/ui/skeleton'

const RELEASES_PER_PAGE = 10

function AppButton({
  active,
  onSelect,
  project,
}: {
  active: boolean
  onSelect: () => void
  project: Project
}) {
  return (
    <Button
      variant={active ? 'secondary' : 'ghost'}
      className="h-auto w-full justify-start gap-3 px-3 py-2.5 text-left"
      onClick={onSelect}
    >
      <RepositoryAvatar
        fullName={project.repository_full_name ?? project.name}
        avatarUrl={project.repository_avatar_url}
        repositoryId={project.repository_id}
        provider={project.repository_provider}
        size="sm"
      />
      <span className="min-w-0 flex-1 truncate">{project.name}</span>
      <HugeiconsIcon
        icon={ArrowRight01Icon}
        className="shrink-0 text-muted-foreground"
      />
    </Button>
  )
}

function QaActivityRow({
  artifactState,
  artifacts,
  build,
  isLatestInstallable,
  versionBase,
}: {
  artifactState: 'error' | 'loading' | 'not_applicable' | 'resolved'
  artifacts: Array<Artifact>
  build: Build
  isLatestInstallable: boolean
  versionBase: string | null
}) {
  const device = detectInstallDevice(
    typeof navigator === 'undefined' ? '' : navigator.userAgent,
  )
  const artifact = selectInstallArtifact(artifacts, device)
  const version = qaBuildVersion(build, artifacts, versionBase)
  const isActive = ['queued', 'scheduled', 'assigned', 'running'].includes(
    build.status,
  )
  const guidance = (() => {
    if (build.changelog) return changelogSummary(build.changelog)
    if (isActive) return 'Build progress and logs are available.'
    if (build.status !== 'succeeded') {
      return 'This build did not produce an installable release.'
    }
    if (artifactState === 'loading') {
      return 'Checking for installable artifacts…'
    }
    if (artifactState === 'error') {
      return 'Artifact availability could not be verified.'
    }
    return artifact
      ? 'Ready to install.'
      : 'No installable artifact was published.'
  })()

  return (
    <Link
      to="/builds/$buildId"
      params={{ buildId: build.id }}
      search={artifact ? { install: artifact.id } : {}}
      resetScroll
      className="flex min-h-16 items-center justify-between gap-3 px-3 py-3 transition-colors hover:bg-muted/50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-inset sm:px-4"
      aria-label={`Open ${version}`}
    >
      <div className="min-w-0">
        <div className="flex flex-wrap items-center gap-2">
          <p className="font-semibold tracking-tight">{version}</p>
          {isLatestInstallable ? (
            <Badge variant="secondary">Latest</Badge>
          ) : null}
          <Badge variant={getStatusVariant(build.status)}>{build.status}</Badge>
        </div>
        <p className="mt-1 line-clamp-1 text-xs text-muted-foreground">
          {guidance}
        </p>
        <p className="mt-1 text-xs text-muted-foreground">
          {relativeTime(build.finished_at ?? build.created_at)}
        </p>
      </div>
      <HugeiconsIcon
        icon={ArrowRight01Icon}
        className="shrink-0 text-muted-foreground"
        aria-hidden
      />
    </Link>
  )
}

function ActivityPanel({ project }: { project: Project }) {
  const [page, setPage] = useState(1)
  const offset = (page - 1) * RELEASES_PER_PAGE
  const buildsQuery = useBuilds({
    project_id: project.id,
    limit: RELEASES_PER_PAGE,
    offset,
  })
  const builds = useMemo(
    () => buildsQuery.data?.builds ?? [],
    [buildsQuery.data?.builds],
  )
  const succeededBuildIds = useMemo(
    () =>
      builds
        .filter((build) => build.status === 'succeeded')
        .map((build) => build.id),
    [builds],
  )
  const artifactsQuery = useArtifactsForBuilds(succeededBuildIds)
  const artifactsByBuild = useMemo(() => {
    const byBuild = new Map<string, Array<Artifact>>()
    for (const artifact of artifactsQuery.data?.artifacts ?? []) {
      const values = byBuild.get(artifact.build_id) ?? []
      values.push(artifact)
      byBuild.set(artifact.build_id, values)
    }
    return byBuild
  }, [artifactsQuery.data?.artifacts])
  const allArtifacts = useMemo(
    () => [...artifactsByBuild.values()].flat(),
    [artifactsByBuild],
  )
  const versionBase = qaProjectVersionBase(allArtifacts)
  const latestInstallableBuildId = builds.find((build) => {
    const artifacts = artifactsByBuild.get(build.id) ?? []
    return selectInstallArtifact(
      artifacts,
      detectInstallDevice(
        typeof navigator === 'undefined' ? '' : navigator.userAgent,
      ),
    )
  })?.id
  const total = buildsQuery.data?.total ?? 0
  const totalPages = Math.max(1, Math.ceil(total / RELEASES_PER_PAGE))
  const paginationItems =
    totalPages <= 5
      ? Array.from({ length: totalPages }, (_, index) => index + 1)
      : page <= 3
        ? [1, 2, 3, 'end', totalPages]
        : page >= totalPages - 2
          ? [1, 'start', totalPages - 2, totalPages - 1, totalPages]
          : [1, 'start', page, 'end', totalPages]
  usePageClamp(page, RELEASES_PER_PAGE, buildsQuery.data?.total, setPage)

  return (
    <Card className="min-w-0 border-0 bg-transparent shadow-none">
      <CardHeader className="flex-row items-start justify-between gap-4">
        <div className="min-w-0 space-y-1.5">
          <div className="flex items-center gap-3">
            <RepositoryAvatar
              fullName={project.repository_full_name ?? project.name}
              avatarUrl={project.repository_avatar_url}
              repositoryId={project.repository_id}
              provider={project.repository_provider}
              size="lg"
            />
            <div className="min-w-0">
              <CardTitle className="truncate text-base font-semibold normal-case tracking-tight text-foreground">
                {project.name}
              </CardTitle>
              <p className="text-xs text-muted-foreground">
                Build activity and versions ready to test
              </p>
            </div>
          </div>
        </div>
        <span className="shrink-0 text-xs text-muted-foreground">
          {total} build{total === 1 ? '' : 's'}
        </span>
      </CardHeader>
      <CardContent>
        {buildsQuery.error ? (
          <Alert variant="destructive">
            <AlertDescription className="flex items-center justify-between gap-3">
              <span>This app’s activity could not be loaded.</span>
              <Button
                variant="outline"
                size="sm"
                onClick={() => void buildsQuery.refetch()}
              >
                <HugeiconsIcon icon={RefreshIcon} />
                Retry
              </Button>
            </AlertDescription>
          </Alert>
        ) : buildsQuery.isLoading ? (
          <div className="space-y-2">
            {Array.from({ length: 4 }).map((_, index) => (
              <Skeleton key={index} className="h-16 w-full" />
            ))}
          </div>
        ) : builds.length === 0 ? (
          <p className="py-6 text-sm text-muted-foreground">
            No builds have been shared for this app yet.
          </p>
        ) : (
          <>
            {artifactsQuery.error && succeededBuildIds.length > 0 ? (
              <Alert variant="destructive" className="mb-3">
                <AlertDescription className="flex items-center justify-between gap-3">
                  <span>Installable artifacts could not be loaded.</span>
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={() => void artifactsQuery.refetch()}
                  >
                    <HugeiconsIcon icon={RefreshIcon} />
                    Retry
                  </Button>
                </AlertDescription>
              </Alert>
            ) : null}
            <div className="-mx-4 divide-y sm:-mx-6">
              {builds.map((build) => (
                <QaActivityRow
                  key={build.id}
                  build={build}
                  artifacts={artifactsByBuild.get(build.id) ?? []}
                  artifactState={
                    build.status !== 'succeeded'
                      ? 'not_applicable'
                      : artifactsQuery.isLoading
                        ? 'loading'
                        : artifactsQuery.error
                          ? 'error'
                          : 'resolved'
                  }
                  versionBase={versionBase}
                  isLatestInstallable={
                    page === 1 && build.id === latestInstallableBuildId
                  }
                />
              ))}
            </div>
          </>
        )}
        {totalPages > 1 ? (
          <Pagination className="mt-4 border-t pt-4">
            <PaginationContent>
              <PaginationItem>
                <PaginationPrevious
                  href="#"
                  aria-disabled={page === 1 || buildsQuery.isFetching}
                  className="aria-disabled:pointer-events-none aria-disabled:opacity-50"
                  onClick={(event) => {
                    event.preventDefault()
                    if (page > 1 && !buildsQuery.isFetching) {
                      setPage((value) => value - 1)
                    }
                  }}
                />
              </PaginationItem>
              {paginationItems.map((item) =>
                typeof item === 'number' ? (
                  <PaginationItem key={item}>
                    <PaginationLink
                      href="#"
                      isActive={page === item}
                      aria-label={`Go to page ${item}`}
                      aria-disabled={buildsQuery.isFetching}
                      className="aria-disabled:pointer-events-none aria-disabled:opacity-50"
                      onClick={(event) => {
                        event.preventDefault()
                        if (!buildsQuery.isFetching) setPage(item)
                      }}
                    >
                      {item}
                    </PaginationLink>
                  </PaginationItem>
                ) : (
                  <PaginationItem key={item}>
                    <PaginationEllipsis />
                  </PaginationItem>
                ),
              )}
              <PaginationItem>
                <PaginationNext
                  href="#"
                  aria-disabled={
                    page === totalPages || buildsQuery.isFetching
                  }
                  className="aria-disabled:pointer-events-none aria-disabled:opacity-50"
                  onClick={(event) => {
                    event.preventDefault()
                    if (page < totalPages && !buildsQuery.isFetching) {
                      setPage((value) => value + 1)
                    }
                  }}
                />
              </PaginationItem>
            </PaginationContent>
          </Pagination>
        ) : null}
      </CardContent>
    </Card>
  )
}

export default function QaReleasesPage() {
  const [search, setSearch] = useState('')
  const [projectSearch, setProjectSearch] = useState('')
  const updateProjectSearch = useDebouncedCallback(setProjectSearch, 300)
  const projectsQuery = useProjectPages({
    search: projectSearch || undefined,
    limit: 20,
    sort: 'name',
    direction: 'asc',
  })
  const projects = useMemo(
    () => projectsQuery.data?.pages.flatMap((page) => page.projects) ?? [],
    [projectsQuery.data?.pages],
  )
  const [selectedProjectId, setSelectedProjectId] = useState<string | null>(
    null,
  )
  const selectedProject =
    projects.find((project) => project.id === selectedProjectId) ??
    projects.at(0)
  const filteredProjects = projects.filter((project) =>
    project.name.toLowerCase().includes(search.trim().toLowerCase()),
  )
  const searchPending =
    search.trim() !== projectSearch || projectsQuery.isFetching

  return (
    <PageLayout width="wide" className="max-w-6xl px-4 py-6 sm:px-6 sm:py-10">
      <PageMeta title="Your apps" noindex />
      <header className="space-y-1">
        <h1 className="text-2xl font-bold tracking-tight sm:text-3xl">
          Your apps
        </h1>
        <p className="text-sm text-muted-foreground">
          Follow build progress, read what changed, and install test versions.
        </p>
      </header>

      {projectsQuery.error ? (
        <Alert variant="destructive">
          <AlertDescription className="flex items-center justify-between gap-3">
            <span>Your apps could not be loaded.</span>
            <Button
              variant="outline"
              size="sm"
              onClick={() => void projectsQuery.refetch()}
            >
              <HugeiconsIcon icon={RefreshIcon} />
              Retry
            </Button>
          </AlertDescription>
        </Alert>
      ) : null}

      {projectsQuery.isLoading ? (
        <div className="grid gap-4 lg:grid-cols-[16rem_minmax(0,1fr)]">
          <Skeleton className="h-72 w-full" />
          <Skeleton className="h-96 w-full" />
        </div>
      ) : null}

      {!projectsQuery.isLoading &&
      !projectsQuery.error &&
      projects.length === 0 ? (
        <div className="border">
          <Empty className="py-12">
            <EmptyHeader>
              <EmptyMedia variant="icon">
                <HugeiconsIcon icon={SmartPhone01Icon} />
              </EmptyMedia>
              <EmptyTitle>No apps shared with you yet</EmptyTitle>
              <EmptyDescription>
                Ask an owner or admin to add you to a project. Its builds will
                appear here automatically.
              </EmptyDescription>
            </EmptyHeader>
          </Empty>
        </div>
      ) : null}

      {selectedProject ? (
        <div className="grid items-start gap-4 lg:grid-cols-[16rem_minmax(0,1fr)]">
          <aside className="flex flex-col gap-3" aria-label="Apps">
            <div className="relative">
              <HugeiconsIcon
                icon={Search01Icon}
                className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-muted-foreground"
                size={16}
              />
              <Input
                value={search}
                onChange={(event) => {
                  const value = event.target.value
                  setSearch(value)
                  updateProjectSearch(value.trim())
                }}
                placeholder="Search apps"
                aria-label="Search apps"
                className="pl-9"
              />
            </div>
            <div className="flex max-h-56 flex-col gap-1 overflow-y-auto lg:max-h-none">
              {filteredProjects.map((project) => (
                <AppButton
                  key={project.id}
                  project={project}
                  active={project.id === selectedProject.id}
                  onSelect={() => setSelectedProjectId(project.id)}
                />
              ))}
              {filteredProjects.length === 0 && searchPending ? (
                <p className="px-3 py-4 text-sm text-muted-foreground">
                  Searching apps…
                </p>
              ) : null}
              {filteredProjects.length === 0 && !searchPending ? (
                <p className="px-3 py-4 text-sm text-muted-foreground">
                  No matching apps.
                </p>
              ) : null}
              {projectsQuery.hasNextPage ? (
                <Button
                  variant="outline"
                  size="sm"
                  disabled={projectsQuery.isFetchingNextPage}
                  onClick={() => void projectsQuery.fetchNextPage()}
                >
                  {projectsQuery.isFetchingNextPage
                    ? 'Loading more…'
                    : 'Load more apps'}
                </Button>
              ) : null}
            </div>
          </aside>

          <ActivityPanel key={selectedProject.id} project={selectedProject} />
        </div>
      ) : null}
    </PageLayout>
  )
}
