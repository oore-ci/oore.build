import { useMemo, useState } from 'react'
import { Link } from '@tanstack/react-router'
import { HugeiconsIcon, type IconSvgElement } from '@hugeicons/react'
import {
  AlertCircleIcon,
  AndroidIcon,
  AppleIcon,
  ArrowDown01Icon,
  ArrowRight01Icon,
  CancelCircleIcon,
  CheckmarkCircle02Icon,
  Clock01Icon,
  Loading03Icon,
  RefreshIcon,
  SmartPhone01Icon,
} from '@hugeicons/core-free-icons'

import type { Artifact, Build, BuildStatus, Release } from '@oore/client/models'
import { useArtifactsForBuilds, useBuilds } from '@/hooks/use-builds'
import { useQaProjectSelection } from '@/hooks/use-qa-project'
import { useReleases } from '@/hooks/use-releases'
import {
  artifactInstallReadiness,
  detectInstallDevice,
  selectInstallArtifact,
} from '@/lib/artifact-install'
import {
  formatDuration,
  formatEnumLabel,
  formatFileSize,
  relativeTime,
} from '@/lib/format-utils'
import {
  changelogSummary,
  qaBuildVersion,
  qaProjectVersionBase,
} from '@/lib/qa-releases'
import { releaseAudienceLabel } from '@/lib/releases'
import { PageMeta } from '@/lib/seo'
import PageLayout from '@/components/page-layout'
import QaBuildStatusBadge from '@/components/qa-build-status-badge'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from '@/components/ui/card'
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible'
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from '@/components/ui/empty'
import {
  Item,
  ItemActions,
  ItemContent,
  ItemDescription,
  ItemGroup,
  ItemMedia,
  ItemTitle,
} from '@/components/ui/item'
import { Skeleton } from '@/components/ui/skeleton'
import { Stack } from '@/components/layout/stack'
import { Text } from '@/components/layout/text'
import { focusRingVariants, rowVariants, textVariants } from '@/lib/recipes'
import { cn } from '@/lib/utils'

const QA_BUILD_WINDOW = 100

function buildArtifacts(
  artifacts: Array<Artifact>,
): Map<string, Array<Artifact>> {
  const byBuild = new Map<string, Array<Artifact>>()
  for (const artifact of artifacts) {
    const values = byBuild.get(artifact.build_id) ?? []
    values.push(artifact)
    byBuild.set(artifact.build_id, values)
  }
  return byBuild
}

function installableArtifacts(artifacts: Array<Artifact>) {
  const now = Math.floor(Date.now() / 1000)
  return artifacts.filter(
    (artifact) =>
      (artifact.artifact_type === 'apk' || artifact.artifact_type === 'ipa') &&
      artifactInstallReadiness(artifact).ready &&
      (artifact.expires_at == null || artifact.expires_at > now),
  )
}

function artifactPlatforms(artifacts: Array<Artifact>) {
  return {
    android: artifacts.find((artifact) => artifact.artifact_type === 'apk'),
    ios: artifacts.find((artifact) => artifact.artifact_type === 'ipa'),
  }
}

function buildDuration(build: Build) {
  if (build.started_at == null || build.finished_at == null) return null
  return formatDuration(build.finished_at - build.started_at)
}

/** Newest store release per build, so QA sees where a version stands. */
function storeReleasesByBuild(releases: Array<Release>) {
  const byBuild = new Map<string, Release>()
  for (const release of [...releases].sort(
    (left, right) => right.updated_at - left.updated_at,
  )) {
    if (!byBuild.has(release.build_id)) byBuild.set(release.build_id, release)
  }
  return byBuild
}

function StoreReleaseBadge({ release }: { release?: Release }) {
  if (!release) return null
  return (
    <Badge
      variant={
        release.state === 'halted' || release.state === 'failed'
          ? 'destructive'
          : 'outline'
      }
    >
      {releaseAudienceLabel(release)}
    </Badge>
  )
}

function CurrentRelease({
  artifacts,
  build,
  storeRelease,
  versionBase,
}: {
  artifacts: Array<Artifact>
  build: Build
  storeRelease?: Release
  versionBase: string | null
}) {
  const version = qaBuildVersion(build, artifacts, versionBase)
  const platforms = artifactPlatforms(installableArtifacts(artifacts))
  const preferredArtifact = selectInstallArtifact(
    artifacts,
    detectInstallDevice(globalThis.navigator?.userAgent ?? ''),
  )
  const releasedAt = build.finished_at ?? build.created_at
  const duration = buildDuration(build)

  return (
    <section aria-labelledby="current-release-title" className="space-y-3">
      <h2 id="current-release-title" className="text-sm font-medium">
        Latest release
      </h2>
      <Card>
        <CardHeader>
          <CardTitle
            className={cn(rowVariants({ gap: 2 }), 'flex-wrap text-lg')}
          >
            {version}
            <QaBuildStatusBadge status={build.status} />
            <StoreReleaseBadge release={storeRelease} />
          </CardTitle>
          <CardDescription>
            Released {relativeTime(releasedAt)}
            {build.branch ? ` · ${build.branch}` : ''}
            {duration ? ` · Built in ${duration}` : ''}
          </CardDescription>
        </CardHeader>
        <CardContent>
          <h3 className="text-sm font-medium">What changed</h3>
          <Text variant="muted" className="mt-1 max-w-2xl leading-relaxed">
            {build.changelog
              ? changelogSummary(build.changelog)
              : 'No release notes were provided for this version.'}
          </Text>
        </CardContent>
        <CardFooter className="flex flex-wrap gap-2">
          {platforms.android ? (
            <>
              <Button
                render={
                  <Link
                    to="/builds/$buildId"
                    params={{ buildId: build.id }}
                    search={{ install: platforms.android.id }}
                    resetScroll
                  />
                }
                nativeButton={false}
                size="icon"
                className="min-w-11 sm:hidden"
                aria-label={`Android${platforms.android.file_size != null ? ` ${formatFileSize(platforms.android.file_size)}` : ''}`}
                variant={
                  preferredArtifact?.artifact_type === 'apk'
                    ? 'default'
                    : 'outline'
                }
              >
                <HugeiconsIcon icon={AndroidIcon} aria-hidden />
              </Button>
              <Button
                render={
                  <Link
                    to="/builds/$buildId"
                    params={{ buildId: build.id }}
                    search={{ install: platforms.android.id }}
                    resetScroll
                  />
                }
                nativeButton={false}
                className="hidden sm:inline-flex"
                variant={
                  preferredArtifact?.artifact_type === 'apk'
                    ? 'default'
                    : 'outline'
                }
              >
                <HugeiconsIcon icon={AndroidIcon} aria-hidden />
                Android
                {platforms.android.file_size != null ? (
                  <span
                    className={
                      preferredArtifact?.artifact_type === 'apk'
                        ? 'text-primary-foreground/80'
                        : 'text-muted-foreground'
                    }
                  >
                    {formatFileSize(platforms.android.file_size)}
                  </span>
                ) : null}
              </Button>
            </>
          ) : null}
          {platforms.ios ? (
            <>
              <Button
                render={
                  <Link
                    to="/builds/$buildId"
                    params={{ buildId: build.id }}
                    search={{ install: platforms.ios.id }}
                    resetScroll
                  />
                }
                nativeButton={false}
                size="icon"
                className="min-w-11 sm:hidden"
                aria-label={`iOS${platforms.ios.file_size != null ? ` ${formatFileSize(platforms.ios.file_size)}` : ''}`}
                variant={
                  preferredArtifact?.artifact_type === 'ipa'
                    ? 'default'
                    : 'outline'
                }
              >
                <HugeiconsIcon icon={AppleIcon} aria-hidden />
              </Button>
              <Button
                render={
                  <Link
                    to="/builds/$buildId"
                    params={{ buildId: build.id }}
                    search={{ install: platforms.ios.id }}
                    resetScroll
                  />
                }
                nativeButton={false}
                className="hidden sm:inline-flex"
                variant={
                  preferredArtifact?.artifact_type === 'ipa'
                    ? 'default'
                    : 'outline'
                }
              >
                <HugeiconsIcon icon={AppleIcon} aria-hidden />
                iOS
                {platforms.ios.file_size != null ? (
                  <span
                    className={
                      preferredArtifact?.artifact_type === 'ipa'
                        ? 'text-primary-foreground/80'
                        : 'text-muted-foreground'
                    }
                  >
                    {formatFileSize(platforms.ios.file_size)}
                  </span>
                ) : null}
              </Button>
            </>
          ) : null}
          <Button
            render={
              <Link
                to="/builds/$buildId"
                params={{ buildId: build.id }}
                search={
                  preferredArtifact ? { install: preferredArtifact.id } : {}
                }
                resetScroll
              />
            }
            nativeButton={false}
            variant="ghost"
            className="ml-auto"
          >
            Details
            <HugeiconsIcon icon={ArrowRight01Icon} />
          </Button>
        </CardFooter>
      </Card>
    </section>
  )
}

const NEWER_BUILD_PREVIEW = 3

const NEWER_BUILD_STATE = {
  succeeded: { icon: CheckmarkCircle02Icon, description: 'Ready to test.' },
  queued: { icon: Clock01Icon, description: 'Waiting to build.' },
  scheduled: { icon: Clock01Icon, description: 'Waiting to build.' },
  assigned: { icon: Clock01Icon, description: 'Starting on a build machine.' },
  running: { icon: Loading03Icon, description: 'Building now.' },
  failed: {
    icon: AlertCircleIcon,
    description: 'This build failed, so there is nothing new to install.',
  },
  timed_out: {
    icon: AlertCircleIcon,
    description: 'This build timed out, so there is nothing new to install.',
  },
  canceled: {
    icon: CancelCircleIcon,
    description: 'This build was canceled.',
  },
  expired: {
    icon: CancelCircleIcon,
    description: 'This build expired before it could run.',
  },
} satisfies Record<BuildStatus, { icon: IconSvgElement; description: string }>

function NewerBuildActivity({
  builds,
  versionBase,
}: {
  builds: Array<Build>
  versionBase: string | null
}) {
  const [showAll, setShowAll] = useState(false)
  if (builds.length === 0) return null
  const visibleBuilds = showAll ? builds : builds.slice(0, NEWER_BUILD_PREVIEW)

  return (
    <section aria-labelledby="newer-builds-title" className="space-y-3">
      <h2 id="newer-builds-title" className="text-sm font-medium">
        Newer build activity
      </h2>
      <ItemGroup className="gap-2">
        {visibleBuilds.map((build) => {
          const state = NEWER_BUILD_STATE[build.status]
          return (
            <Item
              key={build.id}
              render={
                <Link
                  to="/builds/$buildId"
                  params={{ buildId: build.id }}
                  search={{}}
                  resetScroll
                />
              }
              variant="outline"
              size="sm"
            >
              <ItemMedia variant="icon">
                <HugeiconsIcon icon={state.icon} />
              </ItemMedia>
              <ItemContent>
                <ItemTitle>
                  {qaBuildVersion(build, [], versionBase)}
                  <QaBuildStatusBadge status={build.status} />
                </ItemTitle>
                <ItemDescription>
                  {state.description}
                  {build.changelog
                    ? ` ${changelogSummary(build.changelog)}`
                    : ''}
                </ItemDescription>
              </ItemContent>
              <ItemActions>
                <HugeiconsIcon
                  icon={ArrowRight01Icon}
                  className="text-muted-foreground"
                  aria-hidden
                />
              </ItemActions>
            </Item>
          )
        })}
      </ItemGroup>
      {builds.length > NEWER_BUILD_PREVIEW ? (
        <Button
          variant="ghost"
          size="sm"
          aria-expanded={showAll}
          onClick={() => setShowAll((value) => !value)}
        >
          {showAll ? 'Show fewer' : `View all ${builds.length} newer builds`}
          <HugeiconsIcon
            icon={ArrowDown01Icon}
            className={showAll ? 'rotate-180' : undefined}
            aria-hidden
          />
        </Button>
      ) : null}
    </section>
  )
}

function BuildChecks({ build }: { build: Build }) {
  const steps = build.step_results ?? []
  if (steps.length === 0) return null
  const passed = steps.filter((step) => step.status === 'succeeded').length

  return (
    <Collapsible className="border-y">
      <CollapsibleTrigger
        className={cn(
          rowVariants({ gap: 3 }),
          focusRingVariants(),
          'min-h-11 w-full py-2 text-left text-sm',
        )}
      >
        <span className="font-medium">Build checks</span>
        <span className="text-muted-foreground">
          {passed} of {steps.length} completed
        </span>
        <HugeiconsIcon
          icon={ArrowDown01Icon}
          className="ml-auto size-4 text-muted-foreground transition-transform in-data-[open]:rotate-180"
          aria-hidden
        />
      </CollapsibleTrigger>
      <CollapsibleContent className="pb-3">
        <ItemGroup className="gap-1">
          {steps.map((step) => (
            <Item key={step.name} size="xs">
              <ItemMedia variant="icon">
                <HugeiconsIcon
                  icon={
                    step.status === 'succeeded'
                      ? CheckmarkCircle02Icon
                      : Clock01Icon
                  }
                />
              </ItemMedia>
              <ItemContent>
                <ItemTitle>{step.name}</ItemTitle>
              </ItemContent>
              <ItemActions className={textVariants({ variant: 'caption' })}>
                {step.duration_ms != null
                  ? formatDuration(step.duration_ms / 1000)
                  : formatEnumLabel(step.status)}
              </ItemActions>
            </Item>
          ))}
        </ItemGroup>
      </CollapsibleContent>
    </Collapsible>
  )
}

function RecentReleases({
  artifactsByBuild,
  buildsByNewest,
  currentBuildId,
  storeReleases,
  versionBase,
}: {
  artifactsByBuild: Map<string, Array<Artifact>>
  buildsByNewest: Array<Build>
  currentBuildId: string
  storeReleases: Map<string, Release>
  versionBase: string | null
}) {
  const releases = buildsByNewest
    .filter(
      (build) =>
        build.id !== currentBuildId &&
        installableArtifacts(artifactsByBuild.get(build.id) ?? []).length > 0,
    )
    .slice(0, 5)
  if (releases.length === 0) return null

  return (
    <section aria-labelledby="recent-releases-title" className="space-y-3">
      <h2 id="recent-releases-title" className="text-sm font-medium">
        Earlier releases
      </h2>
      <ItemGroup className="gap-2">
        {releases.map((build) => {
          const artifacts = artifactsByBuild.get(build.id) ?? []
          const platforms = artifactPlatforms(installableArtifacts(artifacts))
          const artifact = selectInstallArtifact(artifacts, 'other')
          return (
            <Item
              key={build.id}
              render={
                <Link
                  to="/builds/$buildId"
                  params={{ buildId: build.id }}
                  search={artifact ? { install: artifact.id } : {}}
                  resetScroll
                />
              }
              variant="outline"
              size="sm"
            >
              <ItemContent>
                <ItemTitle>
                  {qaBuildVersion(build, artifacts, versionBase)}
                </ItemTitle>
                <ItemDescription>
                  {relativeTime(build.finished_at ?? build.created_at)}
                  {build.changelog
                    ? ` · ${changelogSummary(build.changelog)}`
                    : ''}
                </ItemDescription>
              </ItemContent>
              <ItemActions>
                <StoreReleaseBadge release={storeReleases.get(build.id)} />
                {platforms.android ? (
                  <Badge variant="outline">Android</Badge>
                ) : null}
                {platforms.ios ? <Badge variant="outline">iOS</Badge> : null}
                <HugeiconsIcon
                  icon={ArrowRight01Icon}
                  className="text-muted-foreground"
                  aria-hidden
                />
              </ItemActions>
            </Item>
          )
        })}
      </ItemGroup>
    </section>
  )
}

function ReleaseWorkspace({
  artifactsByBuild,
  builds,
}: {
  artifactsByBuild: Map<string, Array<Artifact>>
  builds: Array<Build>
}) {
  const projectBuilds = [...builds].sort(
    (left, right) => right.created_at - left.created_at,
  )
  const projectArtifacts = projectBuilds.flatMap(
    (build) => artifactsByBuild.get(build.id) ?? [],
  )
  const versionBase = qaProjectVersionBase(projectArtifacts)
  const projectId = projectBuilds.at(0)?.project_id
  const storeReleasesQuery = useReleases(
    { project_id: projectId, limit: 50 },
    { enabled: !!projectId, select: (data) => data.releases },
  )
  const storeReleases = useMemo(
    () => storeReleasesByBuild(storeReleasesQuery.data ?? []),
    [storeReleasesQuery.data],
  )
  const release = projectBuilds.find(
    (build) =>
      build.status === 'succeeded' &&
      installableArtifacts(artifactsByBuild.get(build.id) ?? []).length > 0,
  )
  const newerBuilds = (
    release
      ? projectBuilds.filter(
          (build) =>
            build.created_at > release.created_at &&
            build.id !== release.id &&
            build.status !== 'succeeded',
        )
      : projectBuilds.filter((build) => build.status !== 'succeeded')
  ).sort((left, right) => right.build_number - left.build_number)

  if (!release) {
    return (
      <Empty className="border py-12">
        <EmptyHeader>
          <EmptyMedia variant="icon">
            <HugeiconsIcon icon={SmartPhone01Icon} />
          </EmptyMedia>
          <EmptyTitle>No release is ready to test</EmptyTitle>
          <EmptyDescription>
            {newerBuilds.length > 0
              ? 'A build is in progress. An install option will appear here when it is ready.'
              : 'No installable Android or iOS release has been shared for this app yet.'}
          </EmptyDescription>
        </EmptyHeader>
        {newerBuilds.at(0) ? (
          <Button
            render={
              <Link
                to="/builds/$buildId"
                params={{ buildId: newerBuilds[0].id }}
                search={{}}
                resetScroll
              />
            }
            nativeButton={false}
            variant="outline"
          >
            View build
          </Button>
        ) : null}
      </Empty>
    )
  }

  return (
    <div className="min-w-0 space-y-8">
      <CurrentRelease
        artifacts={artifactsByBuild.get(release.id) ?? []}
        build={release}
        storeRelease={storeReleases.get(release.id)}
        versionBase={versionBase}
      />
      <NewerBuildActivity builds={newerBuilds} versionBase={versionBase} />
      <BuildChecks build={release} />
      <RecentReleases
        artifactsByBuild={artifactsByBuild}
        buildsByNewest={projectBuilds}
        currentBuildId={release.id}
        storeReleases={storeReleases}
        versionBase={versionBase}
      />
    </div>
  )
}

export default function QaReleasesPage() {
  const selection = useQaProjectSelection()
  const selectedProject = selection.project
  const buildsQuery = useBuilds(
    {
      project_id: selectedProject?.id,
      limit: QA_BUILD_WINDOW,
    },
    { enabled: Boolean(selectedProject) },
  )
  const builds = buildsQuery.data?.builds ?? []
  const succeededBuildIds = builds.flatMap((build) =>
    build.status === 'succeeded' ? [build.id] : [],
  )
  const artifactsQuery = useArtifactsForBuilds(succeededBuildIds)
  const artifactsByBuild = useMemo(
    () => buildArtifacts(artifactsQuery.data?.artifacts ?? []),
    [artifactsQuery.data?.artifacts],
  )
  const loading =
    selection.isLoading ||
    buildsQuery.isLoading ||
    (succeededBuildIds.length > 0 && artifactsQuery.isLoading)
  const error = selection.error ?? buildsQuery.error ?? artifactsQuery.error

  return (
    <PageLayout width="default" className="px-4 py-6 sm:px-6 sm:py-10">
      <PageMeta title="Your apps" noindex />

      {error ? (
        <Alert variant="destructive">
          <AlertDescription className="flex items-center justify-between gap-3">
            <span>Your test releases could not be loaded.</span>
            <Button
              variant="outline"
              size="sm"
              onClick={() => {
                selection.refetch()
                void buildsQuery.refetch()
                void artifactsQuery.refetch()
              }}
            >
              <HugeiconsIcon icon={RefreshIcon} />
              Retry
            </Button>
          </AlertDescription>
        </Alert>
      ) : null}

      {loading ? (
        <Stack gap={4}>
          <Skeleton className="h-6 w-28" />
          <Skeleton className="h-64 w-full" />
        </Stack>
      ) : null}

      {!loading && !error && selection.projectCount === 0 ? (
        <Empty className="border py-12">
          <EmptyHeader>
            <EmptyMedia variant="icon">
              <HugeiconsIcon icon={SmartPhone01Icon} />
            </EmptyMedia>
            <EmptyTitle>No apps shared with you yet</EmptyTitle>
            <EmptyDescription>
              Ask an owner or admin to add you to a project. Its test releases
              will appear here automatically.
            </EmptyDescription>
          </EmptyHeader>
        </Empty>
      ) : null}

      {!loading && !error && selectedProject ? (
        <ReleaseWorkspace artifactsByBuild={artifactsByBuild} builds={builds} />
      ) : null}
    </PageLayout>
  )
}
