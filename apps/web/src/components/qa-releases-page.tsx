import { useMemo, useState } from 'react'
import { Link } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  ArrowRight01Icon,
  FilterIcon,
  RefreshIcon,
  SmartPhone01Icon,
} from '@hugeicons/core-free-icons'

import type { Artifact, Build, BuildStatus, Project } from '@/lib/types'
import { useArtifactsForBuilds, useBuilds } from '@/hooks/use-builds'
import { useProjectPages } from '@/hooks/use-projects'
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
import {
  BUILD_STATUS_FILTER_OPTIONS,
  getStatusVariant,
} from '@/lib/status-variants'
import PageLayout from '@/components/page-layout'
import RepositoryAvatar from '@/components/repository-avatar'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader } from '@/components/ui/card'
import { Checkbox } from '@/components/ui/checkbox'
import {
  Combobox,
  ComboboxContent,
  ComboboxEmpty,
  ComboboxInput,
  ComboboxItem,
  ComboboxList,
} from '@/components/ui/combobox'
import { InputGroupAddon } from '@/components/ui/input-group'
import { Label } from '@/components/ui/label'
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from '@/components/ui/empty'
import {
  Pagination,
  PaginationContent,
  PaginationEllipsis,
  PaginationItem,
  PaginationLink,
  PaginationNext,
  PaginationPrevious,
} from '@/components/ui/pagination'
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetFooter,
  SheetHeader,
  SheetTitle,
  SheetTrigger,
} from '@/components/ui/sheet'
import { Skeleton } from '@/components/ui/skeleton'

const RELEASES_PER_PAGE = 10

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

function ActivityPanel({
  hasMoreProjects,
  isFetchingMoreProjects,
  onLoadMoreProjects,
  onProjectChange,
  project,
  projects,
}: {
  hasMoreProjects: boolean
  isFetchingMoreProjects: boolean
  onLoadMoreProjects: () => void
  onProjectChange: (projectId: string) => void
  project: Project
  projects: Array<Project>
}) {
  const [page, setPage] = useState(1)
  const [statuses, setStatuses] = useState<Array<BuildStatus>>([])
  const [draftStatuses, setDraftStatuses] = useState<Array<BuildStatus>>([])
  const [filterOpen, setFilterOpen] = useState(false)
  const offset = (page - 1) * RELEASES_PER_PAGE
  const buildsQuery = useBuilds({
    project_id: project.id,
    status: statuses.length > 0 ? statuses : undefined,
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
  const filterLabel =
    statuses.length === 0
      ? 'All statuses'
      : statuses.length === 1
        ? BUILD_STATUS_FILTER_OPTIONS[statuses[0]!]
        : `${statuses.length} statuses`
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
    <Card className="min-w-0 bg-transparent shadow-none ring-0">
      <CardHeader className="grid-cols-[minmax(0,1fr)_auto] items-center gap-2 px-0">
        <Combobox
          items={projects}
          value={project}
          onValueChange={(nextProject) => {
            if (nextProject) onProjectChange(nextProject.id)
          }}
          itemToStringLabel={(item) => item.name}
        >
          <ComboboxInput
            className="w-full"
            placeholder="Choose an app"
            aria-label="Choose an app"
          >
            <InputGroupAddon align="inline-start">
              <RepositoryAvatar
                fullName={project.repository_full_name ?? project.name}
                avatarUrl={project.repository_avatar_url}
                repositoryId={project.repository_id}
                provider={project.repository_provider}
                size="sm"
              />
            </InputGroupAddon>
          </ComboboxInput>
          <ComboboxContent>
            <ComboboxEmpty>No matching apps.</ComboboxEmpty>
            <ComboboxList>
              {(item) => (
                <ComboboxItem key={item.id} value={item}>
                  <RepositoryAvatar
                    fullName={item.repository_full_name ?? item.name}
                    avatarUrl={item.repository_avatar_url}
                    repositoryId={item.repository_id}
                    provider={item.repository_provider}
                    size="sm"
                  />
                  <span className="truncate">{item.name}</span>
                </ComboboxItem>
              )}
            </ComboboxList>
            {hasMoreProjects ? (
              <div className="border-t p-2">
                <Button
                  variant="ghost"
                  size="sm"
                  className="w-full"
                  disabled={isFetchingMoreProjects}
                  onClick={onLoadMoreProjects}
                >
                  {isFetchingMoreProjects ? 'Loading more…' : 'Load more apps'}
                </Button>
              </div>
            ) : null}
          </ComboboxContent>
        </Combobox>
        <Sheet
          open={filterOpen}
          onOpenChange={(open) => {
            if (open) setDraftStatuses(statuses)
            setFilterOpen(open)
          }}
        >
          <SheetTrigger
            render={
              <Button
                variant={statuses.length === 0 ? 'outline' : 'secondary'}
                size="icon"
                className="sm:w-auto sm:px-2.5"
                aria-label={`Filter builds: ${filterLabel}`}
              />
            }
          >
            <span className="relative flex">
              <HugeiconsIcon icon={FilterIcon} data-icon="inline-start" />
              {statuses.length > 0 ? (
                <Badge
                  aria-hidden
                  className="absolute -top-1 -right-1 size-2 p-0"
                />
              ) : null}
            </span>
            <span className="hidden sm:inline">
              {statuses.length === 0 ? 'Filter' : filterLabel}
            </span>
          </SheetTrigger>
          <SheetContent
            side="bottom"
            className="max-h-[calc(100dvh_-_1rem_-_var(--safe-area-top))] overflow-y-auto overscroll-contain"
          >
            <SheetHeader className="mx-auto w-full max-w-lg">
              <SheetTitle>Filter builds</SheetTitle>
              <SheetDescription>
                Choose one or more build statuses.
              </SheetDescription>
            </SheetHeader>
            <fieldset className="mx-auto w-full max-w-lg px-4">
              <legend className="sr-only">Build statuses</legend>
              <div className="grid grid-cols-2 gap-x-4">
                {Object.entries(BUILD_STATUS_FILTER_OPTIONS)
                  .filter(([value]) => value !== 'all')
                  .map(([value, label]) => {
                    const buildStatus = value as BuildStatus
                    return (
                      <Label
                        key={value}
                        htmlFor={`qa-build-status-${value}`}
                        className="min-h-11 gap-3"
                      >
                        <Checkbox
                          id={`qa-build-status-${value}`}
                          checked={draftStatuses.includes(buildStatus)}
                          onCheckedChange={(checked) =>
                            setDraftStatuses((current) =>
                              checked
                                ? [...current, buildStatus]
                                : current.filter(
                                    (status) => status !== buildStatus,
                                  ),
                            )
                          }
                        />
                        {label}
                      </Label>
                    )
                  })}
              </div>
            </fieldset>
            <SheetFooter className="mx-auto w-full max-w-lg flex-row">
              <Button
                variant="outline"
                className="flex-1"
                onClick={() => setDraftStatuses([])}
              >
                Clear all
              </Button>
              <Button
                className="flex-1"
                onClick={() => {
                  setStatuses([...draftStatuses].sort())
                  setPage(1)
                  setFilterOpen(false)
                }}
              >
                Apply filters
              </Button>
            </SheetFooter>
          </SheetContent>
        </Sheet>
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
        ) : builds.length === 0 && statuses.length === 0 ? (
          <p className="py-6 text-sm text-muted-foreground">
            No builds have been shared for this app yet.
          </p>
        ) : builds.length === 0 ? (
          <div className="flex flex-col items-start gap-3 py-6">
            <p className="text-sm text-muted-foreground">
              No builds match this status.
            </p>
            <Button
              variant="outline"
              size="sm"
              onClick={() => {
                setStatuses([])
                setPage(1)
              }}
            >
              Clear filter
            </Button>
          </div>
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
                  aria-disabled={page === totalPages || buildsQuery.isFetching}
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
  const projectsQuery = useProjectPages({
    limit: 200,
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

      {projectsQuery.isLoading ? <Skeleton className="h-96 w-full" /> : null}

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
        <ActivityPanel
          key={selectedProject.id}
          project={selectedProject}
          projects={projects}
          onProjectChange={setSelectedProjectId}
          hasMoreProjects={Boolean(projectsQuery.hasNextPage)}
          isFetchingMoreProjects={projectsQuery.isFetchingNextPage}
          onLoadMoreProjects={() => void projectsQuery.fetchNextPage()}
        />
      ) : null}
    </PageLayout>
  )
}
