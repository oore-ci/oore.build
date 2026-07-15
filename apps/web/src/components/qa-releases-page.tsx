import { useMemo, useState } from 'react'
import { Link } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  ArrowRight01Icon,
  Loading03Icon,
  SmartPhone01Icon,
} from '@hugeicons/core-free-icons'

import type { Artifact, Build } from '@/lib/types'
import type { InstallDevice } from '@/lib/artifact-install'
import type { QaRelease } from '@/lib/qa-releases'
import { useArtifactsForBuilds, useBuilds } from '@/hooks/use-builds'
import { useProjects } from '@/hooks/use-projects'
import {
  detectInstallDevice,
  selectInstallArtifact,
} from '@/lib/artifact-install'
import { relativeTime } from '@/lib/format-utils'
import {
  changelogSummary,
  qaBuildVersion,
  qaProjectVersionBase,
  selectQaProjectReleases,
} from '@/lib/qa-releases'
import { PageMeta } from '@/lib/seo'
import PageLayout from '@/components/page-layout'
import RepositoryAvatar from '@/components/repository-avatar'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import {
  Pagination,
  PaginationContent,
  PaginationItem,
  PaginationNext,
  PaginationPrevious,
} from '@/components/ui/pagination'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from '@/components/ui/empty'
import { Skeleton } from '@/components/ui/skeleton'

const RELEASES_PER_PAGE = 10

export function QaReleaseRow({
  device,
  isLatest,
  release,
}: {
  device: InstallDevice
  isLatest: boolean
  release: QaRelease
}) {
  const artifact = selectInstallArtifact(release.artifacts, device)!

  return (
    <Link
      to="/builds/$buildId"
      params={{ buildId: release.build.id }}
      search={{ install: artifact.id }}
      resetScroll
      aria-label={`Open ${release.version}`}
      className="flex min-h-16 items-center justify-between gap-3 px-3 py-2 transition-colors hover:bg-muted/50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-inset sm:px-4"
    >
      <div className="min-w-0">
        <div className="flex flex-wrap items-center gap-2">
          <p className="text-base font-semibold tracking-tight">
            {release.version}
          </p>
          {isLatest ? <Badge variant="secondary">Latest</Badge> : null}
          <span className="text-xs text-muted-foreground">
            {relativeTime(
              release.build.finished_at ?? release.build.created_at,
            )}
          </span>
        </div>
        {release.build.changelog ? (
          <p className="mt-1 line-clamp-1 text-xs text-muted-foreground">
            {changelogSummary(release.build.changelog)}
          </p>
        ) : null}
      </div>
      <HugeiconsIcon
        icon={ArrowRight01Icon}
        className="shrink-0 text-muted-foreground"
        aria-hidden
      />
    </Link>
  )
}

function AppTabPanel({
  activeBuild,
  releases,
  device,
  versionBase,
}: {
  activeBuild?: Build
  releases: Array<QaRelease>
  device: InstallDevice
  versionBase: string | null
}) {
  const [page, setPage] = useState(1)
  const totalPages = Math.max(1, Math.ceil(releases.length / RELEASES_PER_PAGE))
  const currentPage = Math.min(page, totalPages)
  const pageStart = (currentPage - 1) * RELEASES_PER_PAGE
  const visibleReleases = releases.slice(
    pageStart,
    pageStart + RELEASES_PER_PAGE,
  )

  return (
    <section className="space-y-3 pt-3">
      <div className="divide-y border-y">
        {activeBuild ? (
          <div className="flex items-center gap-2.5 px-3 py-3 sm:px-4">
            <HugeiconsIcon
              icon={Loading03Icon}
              className="shrink-0 animate-spin text-info"
              size={16}
            />
            <div className="min-w-0">
              <p className="truncate text-sm">
                <span className="font-medium">
                  {qaBuildVersion(activeBuild, [], versionBase)}
                </span>{' '}
                <span className="text-muted-foreground">is being prepared</span>
              </p>
              {activeBuild.changelog ? (
                <p className="mt-1 line-clamp-1 text-xs text-muted-foreground">
                  {changelogSummary(activeBuild.changelog)}
                </p>
              ) : null}
            </div>
          </div>
        ) : null}
        {visibleReleases.length > 0 ? (
          visibleReleases.map((release, index) => (
            <QaReleaseRow
              key={release.build.id}
              release={release}
              device={device}
              isLatest={pageStart + index === 0}
            />
          ))
        ) : (
          <div className="px-4 py-3">
            <p className="text-sm text-muted-foreground">
              Nothing to install yet. A version will appear automatically.
            </p>
          </div>
        )}
      </div>
      {totalPages > 1 ? (
        <Pagination>
          <PaginationContent>
            {currentPage > 1 ? (
              <PaginationItem>
                <PaginationPrevious
                  href="#"
                  onClick={(event) => {
                    event.preventDefault()
                    setPage(currentPage - 1)
                  }}
                />
              </PaginationItem>
            ) : null}
            <PaginationItem>
              <span className="px-2 text-xs text-muted-foreground">
                {currentPage} of {totalPages}
              </span>
            </PaginationItem>
            {currentPage < totalPages ? (
              <PaginationItem>
                <PaginationNext
                  href="#"
                  onClick={(event) => {
                    event.preventDefault()
                    setPage(currentPage + 1)
                  }}
                />
              </PaginationItem>
            ) : null}
          </PaginationContent>
        </Pagination>
      ) : null}
    </section>
  )
}

export default function QaReleasesPage() {
  const projectsQuery = useProjects({ limit: 200 })
  const buildsQuery = useBuilds({ limit: 200 })
  const projects = useMemo(
    () => projectsQuery.data?.projects ?? [],
    [projectsQuery.data?.projects],
  )
  const builds = useMemo(
    () => buildsQuery.data?.builds ?? [],
    [buildsQuery.data?.builds],
  )
  const buildIds = useMemo(() => builds.map((build) => build.id), [builds])
  const artifactsQuery = useArtifactsForBuilds(buildIds)
  const { activeBuildByProject, releasesByProject, versionByProject } =
    useMemo(() => {
      const projectIdByBuild = new Map(
        builds.map((build) => [build.id, build.project_id]),
      )
      const artifactsByProject = new Map<string, Array<Artifact>>(
        projects.map((project) => [project.id, []]),
      )
      for (const artifact of artifactsQuery.data?.artifacts ?? []) {
        const projectId = projectIdByBuild.get(artifact.build_id)
        if (projectId) artifactsByProject.get(projectId)?.push(artifact)
      }

      const activeBuilds = new Map<string, Build>()
      for (const build of builds) {
        if (
          ['queued', 'scheduled', 'assigned', 'running'].includes(
            build.status,
          ) &&
          (!activeBuilds.has(build.project_id) ||
            build.created_at > activeBuilds.get(build.project_id)!.created_at)
        ) {
          activeBuilds.set(build.project_id, build)
        }
      }

      return {
        activeBuildByProject: activeBuilds,
        releasesByProject: new Map(
          projects.map((project) => [
            project.id,
            selectQaProjectReleases(
              project.id,
              builds,
              artifactsByProject.get(project.id) ?? [],
            ),
          ]),
        ),
        versionByProject: new Map(
          projects.map((project) => [
            project.id,
            qaProjectVersionBase(artifactsByProject.get(project.id) ?? []),
          ]),
        ),
      }
    }, [artifactsQuery.data?.artifacts, builds, projects])
  const device = detectInstallDevice(
    typeof navigator === 'undefined' ? '' : navigator.userAgent,
  )
  const isLoading =
    projectsQuery.isLoading ||
    buildsQuery.isLoading ||
    (buildIds.length > 0 && artifactsQuery.isLoading)
  const error = projectsQuery.error ?? buildsQuery.error ?? artifactsQuery.error

  return (
    <PageLayout width="wide" className="max-w-4xl px-4 py-6 sm:px-6 sm:py-10">
      <PageMeta title="Your apps" noindex />
      <header className="space-y-1">
        <h1 className="text-2xl font-bold tracking-tight sm:text-3xl">
          Your apps
        </h1>
        <p className="text-sm text-muted-foreground">
          Versions ready for you to test.
        </p>
      </header>

      {error ? (
        <Alert variant="destructive">
          <AlertDescription>
            Your apps could not be loaded. Refresh the page to try again.
          </AlertDescription>
        </Alert>
      ) : null}

      {isLoading ? (
        <div className="space-y-4">
          <Skeleton className="h-64 w-full" />
          <Skeleton className="h-64 w-full" />
        </div>
      ) : null}

      {!isLoading && !error && projects.length === 0 ? (
        <div className="border">
          <Empty className="py-12">
            <EmptyHeader>
              <EmptyMedia variant="icon">
                <HugeiconsIcon icon={SmartPhone01Icon} />
              </EmptyMedia>
              <EmptyTitle>No apps shared with you yet</EmptyTitle>
              <EmptyDescription>
                Ask an owner or admin to add you to an app. It will appear here
                as soon as a version is ready.
              </EmptyDescription>
            </EmptyHeader>
          </Empty>
        </div>
      ) : null}

      {!isLoading && !error && projects.length > 0 ? (
        <Tabs defaultValue={projects[0].id} aria-label="Apps">
          <div className="no-scrollbar -mx-4 overflow-x-auto px-4 sm:mx-0 sm:px-0">
            <TabsList variant="line" className="min-w-max justify-start">
              {projects.map((project) => (
                <TabsTrigger key={project.id} value={project.id}>
                  <RepositoryAvatar
                    fullName={project.repository_full_name ?? project.name}
                    avatarUrl={project.repository_avatar_url}
                    repositoryId={project.repository_id}
                    provider={project.repository_provider}
                    size="sm"
                  />
                  {project.name}
                </TabsTrigger>
              ))}
            </TabsList>
          </div>
          {projects.map((project) => (
            <TabsContent key={project.id} value={project.id}>
              <AppTabPanel
                activeBuild={activeBuildByProject.get(project.id)}
                releases={releasesByProject.get(project.id) ?? []}
                device={device}
                versionBase={versionByProject.get(project.id) ?? null}
              />
            </TabsContent>
          ))}
        </Tabs>
      ) : null}
    </PageLayout>
  )
}
