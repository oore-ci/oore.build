import { lazy, Suspense, useCallback, useMemo, useState } from 'react'
import { Link, useNavigate } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  InformationCircleIcon,
  Refresh01Icon,
} from '@hugeicons/core-free-icons'
import { toast } from '@/lib/toast'

import { ArtifactsPanel } from './artifacts-panel'
import { BuildSummary } from './build-summary'
import { EventTimeline } from './event-timeline'
import type { BuildLogChunk } from '@/lib/types'
import { useBreadcrumbLabel } from '@/hooks/use-breadcrumb-label'
import { useBuildNotification } from '@/hooks/use-build-notification'
import { useIsBelowBreakpoint } from '@/hooks/use-mobile'
import {
  isTerminalStatus,
  useArtifacts,
  useBuild,
  useBuildLogs,
  useCancelBuild,
  useRerunBuild,
} from '@/hooks/use-builds'
import { useLogStream } from '@/hooks/use-log-stream'
import { hasProjectPermission, useHasPermission } from '@/hooks/use-permissions'
import { useProject } from '@/hooks/use-projects'
import { mergeBuildLogSnapshots } from '@/lib/log-stream-utils'
import { ApiClientError } from '@/lib/api'
import { PageMeta } from '@/lib/seo'
import { getStatusVariant } from '@/lib/status-variants'
import { cn } from '@/lib/utils'
import PageHeader from '@/components/page-header'
import PageLayout from '@/components/page-layout'
import TerminalLogViewer from '@/components/terminal-log-viewer'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Skeleton } from '@/components/ui/skeleton'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'

const loadCancelBuildDialog = () => import('./cancel-build-dialog')
const CancelBuildDialog = lazy(loadCancelBuildDialog)

export function BuildDetailPage({ buildId }: { buildId: string }) {
  const navigate = useNavigate()
  const usesTabbedArtifacts = useIsBelowBreakpoint(1280)
  const canTriggerBuildGlobally = useHasPermission('builds', 'write')
  const canCancelBuildGlobally = useHasPermission('builds', 'cancel')
  const canManageShareLinksGlobally = useHasPermission('artifacts', 'write')
  const rerunMutation = useRerunBuild()
  const buildQuery = useBuild(buildId, {
    refetchInterval: (query) =>
      query.state.data && isTerminalStatus(query.state.data.build.status)
        ? false
        : 3000,
  })
  const { data, isLoading, error, refetch: refetchBuild } = buildQuery
  const projectQuery = useProject(data?.build.project_id ?? '')
  const projectRole = projectQuery.data?.project.current_user_role
  const canTriggerBuild =
    canTriggerBuildGlobally &&
    hasProjectPermission(projectRole, 'builds', 'write')
  const canCancelBuild =
    canCancelBuildGlobally &&
    hasProjectPermission(projectRole, 'builds', 'cancel')
  const canManageShareLinks =
    canManageShareLinksGlobally &&
    hasProjectPermission(projectRole, 'artifacts', 'write')
  const buildStatus = data?.build.status
  const isTerminal = buildStatus ? isTerminalStatus(buildStatus) : false
  const artifactsQuery = useArtifacts(buildId, {
    refetchInterval: isTerminal ? false : 3000,
  })
  const { refetch: refetchArtifacts } = artifactsQuery
  const cancelMutation = useCancelBuild()
  const [cancelOpen, setCancelOpen] = useState(false)

  const label = data?.build.build_number
    ? `Build #${data.build.build_number}`
    : 'Build Details'

  useBreadcrumbLabel(
    '/builds/$buildId',
    data?.build.build_number ? `Build #${data.build.build_number}` : undefined,
  )

  useBuildNotification(data?.build, isTerminal)

  const streamEnabled = !isTerminal
  const { logs: streamLogs, isStreaming } = useLogStream(
    buildId,
    streamEnabled,
    {
      onDone: useCallback(() => {
        void refetchBuild()
        void refetchArtifacts()
      }, [refetchBuild, refetchArtifacts]),
    },
  )
  const fullLogsQuery = useBuildLogs(buildId, { enabled: isTerminal })
  const { data: fullLogsData } = fullLogsQuery

  const mergedLogs: Array<BuildLogChunk> = useMemo(() => {
    return mergeBuildLogSnapshots(streamLogs, fullLogsData?.logs ?? [])
  }, [streamLogs, fullLogsData?.logs])

  function handleCancel() {
    cancelMutation.mutate(buildId, {
      onSuccess: () => {
        toast.success('Build canceled')
        setCancelOpen(false)
      },
      onError: (err) => {
        toast.error(`Failed to cancel: ${err.message}`)
      },
    })
  }

  if (isLoading) {
    return (
      <PageLayout width="full">
        <PageMeta title={label} noindex />
        <Skeleton className="h-8 w-56" />
        <Skeleton className="h-24 w-full" />
        <Skeleton className="h-64 w-full" />
      </PageLayout>
    )
  }

  if (error) {
    const notFound = error instanceof ApiClientError && error.status === 404
    return (
      <PageLayout width="full">
        <PageMeta title={label} noindex />
        <Alert variant="destructive">
          <HugeiconsIcon icon={InformationCircleIcon} size={16} />
          <AlertDescription className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
            <span>
              {notFound
                ? 'This build was not found or is no longer available.'
                : `Failed to load build: ${error.message}`}
            </span>
            <span className="flex gap-2">
              <Button
                variant="outline"
                size="sm"
                render={<Link to="/builds" />}
              >
                Back to builds
              </Button>
              {!notFound ? (
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => void refetchBuild()}
                >
                  Retry
                </Button>
              ) : null}
            </span>
          </AlertDescription>
        </Alert>
      </PageLayout>
    )
  }

  if (!data) return null

  const { build, events } = data
  const canCancel = !isTerminal && canCancelBuild
  const duration = build.started_at
    ? (build.finished_at ?? Math.floor(Date.now() / 1000)) - build.started_at
    : null
  const failureReason =
    build.status === 'failed'
      ? ([...events].reverse().find((event) => event.reason)?.reason ??
        `Build failed${build.exit_code != null ? ` with exit code ${build.exit_code}` : ''}.`)
      : build.status === 'timed_out'
        ? ([...events].reverse().find((event) => event.reason)?.reason ??
          'Build timed out.')
        : build.status === 'canceled'
          ? ([...events].reverse().find((event) => event.reason)?.reason ??
            'Build was canceled.')
          : undefined

  return (
    <PageLayout
      width="full"
      className={cn(
        usesTabbedArtifacts &&
          'flex min-h-0 flex-1 flex-col gap-6 space-y-0 overflow-hidden pb-6',
      )}
    >
      <PageMeta title={label} noindex />
      <PageHeader
        title={`Build #${build.build_number}`}
        description={
          [build.context?.project_name, build.context?.pipeline_name]
            .filter(Boolean)
            .join(' · ') || undefined
        }
        meta={
          <>
            <Badge variant={getStatusVariant(build.status)}>
              {build.status}
            </Badge>
            <Badge variant="outline">{build.trigger_type}</Badge>
            <span aria-hidden className="h-3 w-px bg-border" />
            <BuildSummary build={build} duration={duration} />
          </>
        }
        actions={
          <div className="flex items-center gap-2">
            {isTerminal && canTriggerBuild ? (
              <Button
                variant="outline"
                size="sm"
                onClick={() => {
                  rerunMutation.mutate(build.id, {
                    onSuccess: (result) => {
                      toast.success(
                        `Re-run queued as build #${result.build.build_number}`,
                      )
                      void navigate({
                        to: '/builds/$buildId',
                        params: { buildId: result.build.id },
                      })
                    },
                    onError: (err) => {
                      toast.error(`Failed to re-run: ${err.message}`)
                    },
                  })
                }}
                disabled={rerunMutation.isPending}
              >
                <HugeiconsIcon icon={Refresh01Icon} size={14} />
                {rerunMutation.isPending ? 'Re-running...' : 'Re-run'}
              </Button>
            ) : null}
            {canCancel ? (
              <Button
                variant="destructive"
                size="sm"
                onMouseEnter={() => void loadCancelBuildDialog()}
                onFocus={() => void loadCancelBuildDialog()}
                onClick={() => setCancelOpen(true)}
                disabled={cancelMutation.isPending}
              >
                {cancelMutation.isPending ? 'Canceling...' : 'Cancel Build'}
              </Button>
            ) : null}
          </div>
        }
      />

      {build.runner_policy_block_reason ? (
        <Alert>
          <HugeiconsIcon icon={InformationCircleIcon} size={16} />
          <AlertDescription>
            {build.runner_policy_block_reason === 'instance_disabled' ? (
              <>
                This build is waiting because the Direct macOS runner is paused.
                An owner or admin can enable it in{' '}
                <Link
                  to="/settings/runners"
                  className="font-medium underline underline-offset-4"
                >
                  Runners
                </Link>
                .
              </>
            ) : build.runner_policy_block_reason ===
              'repository_not_approved' ? (
              <>
                This build is waiting for repository approval. An owner or admin
                can approve it under{' '}
                <Link
                  to="/settings/integrations"
                  className="font-medium underline underline-offset-4"
                >
                  Sources
                </Link>
                .
              </>
            ) : (
              <>
                This build is waiting because its repository policy is
                unavailable. Check the project&apos;s repository under{' '}
                <Link
                  to="/settings/integrations"
                  className="font-medium underline underline-offset-4"
                >
                  Sources
                </Link>
                .
              </>
            )}
          </AlertDescription>
        </Alert>
      ) : null}

      {failureReason ? (
        <Alert variant="destructive">
          <HugeiconsIcon icon={InformationCircleIcon} size={16} />
          <AlertDescription>{failureReason}</AlertDescription>
        </Alert>
      ) : null}

      <Tabs
        key={usesTabbedArtifacts ? 'compact' : 'desktop'}
        defaultValue="logs"
        className={cn('gap-3', usesTabbedArtifacts && 'min-h-0 flex-1')}
      >
        <TabsList variant="line">
          <TabsTrigger value="logs">Logs</TabsTrigger>
          <TabsTrigger value="timeline">
            Timeline{events.length > 0 ? ` (${events.length})` : ''}
          </TabsTrigger>
          {usesTabbedArtifacts ? (
            <TabsTrigger value="artifacts">
              Artifacts
              {artifactsQuery.data?.artifacts.length
                ? ` (${artifactsQuery.data.artifacts.length})`
                : ''}
            </TabsTrigger>
          ) : null}
        </TabsList>
        <div
          className={cn(
            'grid min-w-0 items-start gap-6 xl:grid-cols-[minmax(0,1fr)_22rem]',
            usesTabbedArtifacts && 'min-h-0 flex-1',
          )}
        >
          <div
            className={cn('min-w-0', usesTabbedArtifacts && 'h-full min-h-0')}
          >
            <TabsContent
              value="logs"
              className={cn(usesTabbedArtifacts && 'h-full min-h-0')}
            >
              <TerminalLogViewer
                logs={mergedLogs}
                stepResults={build.step_results ?? []}
                isStreaming={isStreaming && !isTerminal}
                fillAvailableHeight={usesTabbedArtifacts}
                isLoading={isTerminal && fullLogsQuery.isLoading}
                logsUnavailable={fullLogsQuery.isError}
                isTerminal={isTerminal}
              />
            </TabsContent>
            <TabsContent value="timeline">
              <EventTimeline events={events} />
            </TabsContent>
            {usesTabbedArtifacts ? (
              <TabsContent value="artifacts">
                <ArtifactsPanel
                  artifacts={artifactsQuery.data?.artifacts ?? []}
                  isLoading={artifactsQuery.isLoading}
                  buildStatus={build.status}
                  canManageShareLinks={canManageShareLinks}
                />
              </TabsContent>
            ) : null}
          </div>
          {!usesTabbedArtifacts ? (
            <aside aria-label="Build output" className="sticky top-6">
              <ArtifactsPanel
                artifacts={artifactsQuery.data?.artifacts ?? []}
                isLoading={artifactsQuery.isLoading}
                buildStatus={build.status}
                canManageShareLinks={canManageShareLinks}
              />
            </aside>
          ) : null}
        </div>
      </Tabs>

      {cancelOpen ? (
        <Suspense
          fallback={
            <span className="sr-only" role="status">
              Loading cancel confirmation
            </span>
          }
        >
          <CancelBuildDialog
            buildNumber={build.build_number}
            isPending={cancelMutation.isPending}
            onCancel={handleCancel}
            onOpenChange={setCancelOpen}
            open
          />
        </Suspense>
      ) : null}
    </PageLayout>
  )
}
