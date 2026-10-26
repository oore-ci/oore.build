import { useEffect, useId, useMemo, type ReactNode } from 'react'
import { Link } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import { Download04Icon } from '@hugeicons/core-free-icons'
import type { Artifact, Build, Pipeline } from '@oore/client/models'
import { useProjectArtifacts } from '@/hooks/use-builds'
import { BuildItem } from '@/components/build-item'
import { ReleaseActivity } from '@/components/releases/release-activity'
import { ReleaseBoard } from '@/components/releases/release-board'
import { StatusBadge } from '@/components/status-badge'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  Item,
  ItemActions,
  ItemContent,
  ItemDescription,
  ItemGroup,
  ItemTitle,
} from '@/components/ui/item'
import { Skeleton } from '@/components/ui/skeleton'
import { latestAppPerPlatform } from '@/lib/artifact-install'
import { BUILD_PLATFORM_LABELS } from '@/lib/build-platforms'
import { formatFileSize, relativeTime } from '@/lib/format-utils'
import type { LastPipelineBuild } from './-project-build-summary'
import { Row } from '@/components/layout/row'
import { Stack } from '@/components/layout/stack'
import { Text } from '@/components/layout/text'
import { textVariants } from '@/lib/recipes'
import { cn } from '@/lib/utils'

const OVERVIEW_PIPELINE_LIMIT = 5

/** One compact heading treatment, with an optional `View all` beside it. */
function OverviewSection({
  action,
  children,
  title,
}: {
  action?: ReactNode
  children: ReactNode
  title: string
}) {
  const titleId = useId()
  return (
    <section aria-labelledby={titleId} className="flex flex-col gap-3">
      <Row gap={3} className="justify-between">
        <h2
          id={titleId}
          className={cn(textVariants({ variant: 'muted' }), 'font-medium')}
        >
          {title}
        </h2>
        {action}
      </Row>
      {children}
    </section>
  )
}

function ViewAllLink({
  projectId,
  tab,
}: {
  projectId: string
  tab: 'builds' | 'pipelines'
}) {
  return (
    <Button
      variant="ghost"
      size="sm"
      className="-my-1.5"
      render={
        <Link
          to="/projects/$projectId"
          params={{ projectId }}
          search={{ tab }}
        />
      }
      nativeButton={false}
    >
      View all
    </Button>
  )
}

function PipelineSummary({
  lastBuildByPipeline,
  pipelines,
  projectId,
}: {
  lastBuildByPipeline: ReadonlyMap<string, LastPipelineBuild>
  pipelines: ReadonlyArray<Pipeline>
  projectId: string
}) {
  return (
    <ItemGroup className="gap-2">
      {pipelines.slice(0, OVERVIEW_PIPELINE_LIMIT).map((pipeline) => {
        const lastBuild = lastBuildByPipeline.get(pipeline.id)
        return (
          <Item
            key={pipeline.id}
            variant="outline"
            size="sm"
            render={
              <Link
                to="/projects/$projectId/pipelines/$pipelineId"
                params={{ projectId, pipelineId: pipeline.id }}
              />
            }
          >
            <ItemContent className="min-w-0">
              <ItemTitle>{pipeline.name}</ItemTitle>
              <ItemDescription>
                {pipeline.execution_config.platforms
                  .map((platform) => BUILD_PLATFORM_LABELS[platform])
                  .join(', ')}
                {pipeline.enabled ? '' : ' · Disabled'}
              </ItemDescription>
            </ItemContent>
            <ItemActions>
              {lastBuild ? (
                <>
                  <span className={textVariants({ variant: 'caption' })}>
                    #{lastBuild.buildNumber} · {relativeTime(lastBuild.time)}
                  </span>
                  <StatusBadge kind="build" value={lastBuild.status} />
                </>
              ) : (
                <span className={textVariants({ variant: 'caption' })}>
                  No recent builds
                </span>
              )}
            </ItemActions>
          </Item>
        )
      })}
    </ItemGroup>
  )
}

function LatestApps({
  artifacts,
  projectName,
}: {
  artifacts: ReadonlyArray<Artifact>
  projectName: string
}) {
  const apps = useMemo(() => latestAppPerPlatform(artifacts), [artifacts])

  if (apps.length === 0) {
    return (
      <Text variant="muted">
        No installable app in recent builds. Check output paths and iOS signing
        in the build details.
      </Text>
    )
  }

  return (
    <ItemGroup className="gap-2">
      {apps.map(({ artifact, label }) => (
        <Item key={artifact.id} variant="outline" size="sm">
          <ItemContent className="min-w-0">
            <ItemTitle>
              {projectName}
              <Badge variant="secondary">{label}</Badge>
            </ItemTitle>
            <ItemDescription className="truncate">
              {artifact.name}
              {artifact.file_size != null
                ? ` · ${formatFileSize(artifact.file_size)}`
                : ''}
              {` · ${relativeTime(artifact.created_at)}`}
            </ItemDescription>
          </ItemContent>
          <ItemActions>
            <Button
              variant="outline"
              size="sm"
              render={
                <Link
                  to="/builds/$buildId"
                  params={{ buildId: artifact.build_id }}
                  search={{ install: artifact.id }}
                  aria-label={`Install ${projectName} for ${label}`}
                />
              }
              nativeButton={false}
            >
              <HugeiconsIcon icon={Download04Icon} data-icon="inline-start" />
              Install
            </Button>
          </ItemActions>
        </Item>
      ))}
    </ItemGroup>
  )
}

export default function ProjectOverview({
  projectId,
  projectName,
  builds,
  lastBuildByPipeline,
  loading,
  error,
  onRetry,
  pipelines,
  setup,
}: {
  projectId: string
  projectName: string
  builds: Build[]
  lastBuildByPipeline: ReadonlyMap<string, LastPipelineBuild>
  loading: boolean
  error: Error | null
  onRetry: () => void
  pipelines: ReadonlyArray<Pipeline>
  /** The setup checklist, rendered first while the project has no app yet. */
  setup?: ReactNode
}) {
  const artifacts = useProjectArtifacts(projectId)
  const { refetch } = artifacts
  const latest = builds[0]
  useEffect(() => {
    if (latest?.status === 'succeeded') void refetch()
  }, [latest?.id, latest?.status, refetch])
  const showApps = !(setup && !loading && builds.length === 0)

  return (
    <Stack gap={8} className="pt-3">
      {setup}
      {pipelines.length > 0 ? (
        <OverviewSection
          title="Pipelines"
          action={
            pipelines.length > OVERVIEW_PIPELINE_LIMIT ? (
              <ViewAllLink projectId={projectId} tab="pipelines" />
            ) : undefined
          }
        >
          <PipelineSummary
            lastBuildByPipeline={lastBuildByPipeline}
            pipelines={pipelines}
            projectId={projectId}
          />
        </OverviewSection>
      ) : null}
      <ReleaseBoard projectId={projectId} />
      {showApps ? (
        <OverviewSection title="Latest apps">
          {artifacts.error ? (
            <Alert>
              <AlertDescription className="flex flex-wrap items-center justify-between gap-3">
                Could not load apps.
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => void artifacts.refetch()}
                >
                  Retry
                </Button>
              </AlertDescription>
            </Alert>
          ) : artifacts.isLoading ? (
            <Skeleton className="h-32 w-full" />
          ) : (
            <LatestApps
              artifacts={artifacts.data?.artifacts ?? []}
              projectName={projectName}
            />
          )}
        </OverviewSection>
      ) : null}
      <OverviewSection
        title="Recent activity"
        action={
          builds.length > 0 ? (
            <ViewAllLink projectId={projectId} tab="builds" />
          ) : undefined
        }
      >
        {error ? (
          <Alert>
            <AlertDescription className="flex flex-wrap items-center justify-between gap-3">
              Could not load recent activity.
              <Button variant="outline" size="sm" onClick={onRetry}>
                Retry
              </Button>
            </AlertDescription>
          </Alert>
        ) : loading ? (
          <Skeleton className="h-40 w-full" />
        ) : builds.length ? (
          <Stack role="list" gap={2}>
            {builds.slice(0, 5).map((build) => (
              <BuildItem key={build.id} build={build} />
            ))}
          </Stack>
        ) : (
          <Text variant="muted">
            {setup
              ? 'Builds show up here after the first run.'
              : 'No builds yet. Add a pipeline, then run a build.'}
          </Text>
        )}
      </OverviewSection>
      <ReleaseActivity projectId={projectId} />
    </Stack>
  )
}
