import { lazy, Suspense, useMemo, useState } from 'react'
import { Link, useNavigate } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  InformationCircleIcon,
  Refresh01Icon,
} from '@hugeicons/core-free-icons'
import { toast } from '@/lib/toast'

import { ArtifactsPanel } from './artifacts-panel'
import { AppOutputs } from './app-outputs'
import { buildStateDescription } from '@/lib/build-state-description'
import { BuildSummary } from './build-summary'
import { EventTimeline } from './event-timeline'
import { TestsPanel } from './tests-panel'
import type { BuildLogChunk } from '@oore/client/models'
import { useBuildNotification } from '@/hooks/use-build-notification'
import { useIsBelowBreakpoint } from '@/hooks/use-mobile'
import {
  isTerminalStatus,
  useArtifacts,
  useBuild,
  useBuildApprovalDecision,
  useBuildLogs,
  useBuildTests,
  useCancelBuild,
  useRerunBuild,
} from '@/hooks/use-builds'
import { useLogStream } from '@/hooks/use-log-stream'
import { useReleases } from '@/hooks/use-releases'
import { ReleasePanel } from '@/components/releases/release-panel'
import {
  hasProjectPermission,
  useHasPermissions,
} from '@/hooks/use-permissions'
import { useProject } from '@/hooks/use-projects'
import { mergeBuildLogSnapshots } from '@/lib/log-stream-utils'
import { isOoreApiError } from '@oore/client/client'
import { PageMeta } from '@/lib/seo'
import { TRIGGER_TYPE_LABELS } from '@/lib/status-variants'
import { StatusBadge } from '@/components/status-badge'
import { cn } from '@/lib/utils'
import PageHeader from '@/components/page-header'
import PageLayout from '@/components/page-layout'
import TerminalLogViewer from '@/components/terminal-log-viewer'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Skeleton } from '@/components/ui/skeleton'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { useTime } from '@/hooks/use-time'
import { LIVE_FALLBACK_POLL_MS } from '@/lib/live-events'
import { useLiveEventsStore } from '@/stores/live-events-store'

const loadCancelBuildDialog = () => import('./cancel-build-dialog')
const CancelBuildDialog = lazy(loadCancelBuildDialog)
const loadRejectBuildDialog = () => import('./reject-build-dialog')
const RejectBuildDialog = lazy(loadRejectBuildDialog)

export function BuildDetailPage({
  buildId,
  initialTab,
  selectedReleaseId,
}: {
  buildId: string
  initialTab?: string
  selectedReleaseId?: string
}) {
  const time = useTime()
  const navigate = useNavigate()
  const usesTabbedArtifacts = useIsBelowBreakpoint(1280)
  const [
    canTriggerBuildGlobally,
    canCancelBuildGlobally,
    canManageShareLinksGlobally,
    canWriteInstanceSettings,
  ] = useHasPermissions([
    'builds:write',
    'builds:cancel',
    'artifacts:write',
    'instance_settings:write',
  ])
  const rerunMutation = useRerunBuild()
  const liveEvents = useLiveEventsStore((s) => s.connected)
  const buildQuery = useBuild(buildId, {
    refetchInterval: (query) =>
      query.state.data && isTerminalStatus(query.state.data.build.status)
        ? false
        : liveEvents
          ? LIVE_FALLBACK_POLL_MS
          : 3000,
  })
  const { data, isLoading, error, refetch: refetchBuild } = buildQuery
  const projectQuery = useProject(data?.build.project_id ?? '')
  const projectRole = projectQuery.data?.project.current_user_role
  const canTriggerBuild =
    canTriggerBuildGlobally && hasProjectPermission(projectRole, 'builds:write')
  const canCancelBuild =
    canCancelBuildGlobally && hasProjectPermission(projectRole, 'builds:cancel')
  const canApproveBuild = hasProjectPermission(projectRole, 'builds:approve')
  const canManageShareLinks =
    canManageShareLinksGlobally &&
    hasProjectPermission(projectRole, 'artifacts:write')
  const buildStatus = data?.build.status
  const isTerminal = buildStatus ? isTerminalStatus(buildStatus) : false
  const artifactsQuery = useArtifacts(buildId, {
    refetchInterval: isTerminal ? false : 3000,
  })
  const artifacts = artifactsQuery.data?.artifacts ?? []
  const { refetch: refetchArtifacts } = artifactsQuery
  const cancelMutation = useCancelBuild()
  const [cancelOpen, setCancelOpen] = useState(false)
  const approveMutation = useBuildApprovalDecision('approve')
  const rejectMutation = useBuildApprovalDecision('reject')
  const [rejectOpen, setRejectOpen] = useState(false)
  const [chosenTab, setActiveTab] = useState<string | number | null>(
    initialTab ?? null,
  )
  const testsQuery = useBuildTests(buildId, {
    refetchInterval: isTerminal ? false : 5000,
  })
  const testSummary = testsQuery.data?.summary
  const hasTestsTab =
    !!testSummary || testsQuery.isError || initialTab === 'tests'
  // A failed build with failing tests opens on Tests until the user picks a tab.
  const activeTab =
    chosenTab ??
    (buildStatus === 'failed' && testSummary?.failed ? 'tests' : 'logs')
  const releasesQuery = useReleases(
    { build_id: buildId },
    { enabled: buildStatus === 'succeeded' },
  )
  const releases = releasesQuery.data?.releases ?? []
  const hasReleaseTab =
    releases.length > 0 || releasesQuery.isError || initialTab === 'release'

  const label = data?.build.build_number
    ? `Build #${data.build.build_number}`
    : 'Build Details'

  useBuildNotification(data?.build, isTerminal)

  const streamEnabled = !isTerminal
  const { logs: streamLogs, isStreaming } = useLogStream(
    buildId,
    streamEnabled,
    {
      onDone: () => {
        void refetchBuild()
        void refetchArtifacts()
      },
    },
  )
  const fullLogsQuery = useBuildLogs(buildId, { enabled: isTerminal })
  const { data: fullLogsData } = fullLogsQuery

  const mergedLogs: Array<BuildLogChunk> = useMemo(() => {
    return mergeBuildLogSnapshots(streamLogs, fullLogsData?.logs ?? [])
  }, [streamLogs, fullLogsData?.logs])

  function handleApprove() {
    approveMutation.mutate(
      { buildId },
      {
        onSuccess: () => toast.success('Build approved'),
        onError: (err) => toast.error(`Failed to approve: ${err.message}`),
      },
    )
  }

  function handleReject(reason: string) {
    rejectMutation.mutate(
      { buildId, reason },
      {
        onSuccess: () => {
          toast.success('Build rejected')
          setRejectOpen(false)
        },
        onError: (err) => toast.error(`Failed to reject: ${err.message}`),
      },
    )
  }

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
    const notFound = isOoreApiError(error) && error.status === 404
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
  const awaitingApproval = build.approval?.state === 'pending'
  const canDecideApproval = awaitingApproval && canApproveBuild
  // A maintainer rejects rather than cancels a build waiting for approval.
  const canCancel = !isTerminal && canCancelBuild && !canDecideApproval
  const duration = build.started_at
    ? (build.finished_at ?? Math.floor(time / 1000)) - build.started_at
    : null
  const stateDescription = buildStateDescription(build, events)

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
            <StatusBadge kind="build" value={build.status} />
            <Badge variant="outline">
              {TRIGGER_TYPE_LABELS[build.trigger_type]}
            </Badge>
            {build.approval ? (
              <StatusBadge kind="approval" value={build.approval.state} />
            ) : null}
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
            {canDecideApproval ? (
              <>
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => setRejectOpen(true)}
                  disabled={
                    approveMutation.isPending || rejectMutation.isPending
                  }
                >
                  Reject
                </Button>
                <Button
                  size="sm"
                  onClick={handleApprove}
                  disabled={
                    approveMutation.isPending || rejectMutation.isPending
                  }
                >
                  {approveMutation.isPending ? 'Approving...' : 'Approve'}
                </Button>
              </>
            ) : null}
            {canCancel ? (
              <Button
                variant="destructive"
                size="sm"
                onClick={() => setCancelOpen(true)}
                disabled={cancelMutation.isPending}
              >
                {cancelMutation.isPending ? 'Canceling...' : 'Cancel build'}
              </Button>
            ) : null}
          </div>
        }
      />

      {build.runner_policy_block_reason ? (
        <Alert>
          <HugeiconsIcon icon={InformationCircleIcon} size={16} />
          <AlertDescription>
            {build.runner_policy_block_reason === 'instance_paused' ? (
              canWriteInstanceSettings ? (
                <>
                  This build is waiting because the Direct macOS runner is
                  paused. Resume it in{' '}
                  <Link
                    to="/settings/preferences"
                    className="font-medium underline underline-offset-4"
                  >
                    Preferences
                  </Link>
                  .
                </>
              ) : (
                <>
                  This build is waiting because the Direct macOS runner is
                  paused. Ask an owner or admin to resume it.
                </>
              )
            ) : (
              <>
                This build is waiting because its source repository is
                unavailable. An owner or admin can repair the source link in{' '}
                <Link
                  to="/projects/$projectId"
                  params={{ projectId: build.project_id }}
                  search={{ tab: 'settings' }}
                  className="font-medium underline underline-offset-4"
                >
                  Project Settings
                </Link>
                .
              </>
            )}
          </AlertDescription>
        </Alert>
      ) : null}

      {stateDescription ? (
        <Alert
          variant={
            isTerminal && build.status !== 'succeeded'
              ? 'destructive'
              : 'default'
          }
        >
          <HugeiconsIcon icon={InformationCircleIcon} size={16} />
          <AlertDescription className="flex flex-col gap-2">
            <p role="status">{stateDescription}</p>
            {isTerminal && build.status !== 'succeeded' ? (
              <p>
                {testSummary?.failed
                  ? `${testSummary.failed} tests failed. Check Tests and Logs before retrying.`
                  : 'Check Logs and Timeline before retrying.'}
              </p>
            ) : null}
          </AlertDescription>
        </Alert>
      ) : null}

      {build.status === 'succeeded' ? (
        artifactsQuery.error ? (
          <Alert>
            <AlertDescription className="flex flex-wrap items-center justify-between gap-3">
              Could not load apps.
              <Button
                variant="outline"
                size="sm"
                onClick={() => void refetchArtifacts()}
              >
                Retry
              </Button>
            </AlertDescription>
          </Alert>
        ) : artifactsQuery.isLoading ? (
          <Skeleton className="h-32 w-full" />
        ) : (
          <AppOutputs
            artifacts={artifacts}
            projectName={build.context?.project_name}
          />
        )
      ) : null}

      <Tabs
        value={
          !usesTabbedArtifacts && activeTab === 'artifacts' ? 'logs' : activeTab
        }
        onValueChange={setActiveTab}
        className={cn('gap-3', usesTabbedArtifacts && 'min-h-0 flex-1')}
      >
        <TabsList variant="line" aria-label="Build details">
          <TabsTrigger value="logs">Logs</TabsTrigger>
          <TabsTrigger value="timeline">
            Timeline{events.length > 0 ? ` (${events.length})` : ''}
          </TabsTrigger>
          {hasTestsTab ? (
            <TabsTrigger value="tests">
              Tests
              {testSummary?.failed
                ? ` (${testSummary.failed} failed)`
                : testSummary
                  ? ` (${testSummary.total})`
                  : ''}
            </TabsTrigger>
          ) : null}
          {usesTabbedArtifacts ? (
            <TabsTrigger value="artifacts">
              Artifacts
              {artifacts.length ? ` (${artifacts.length})` : ''}
            </TabsTrigger>
          ) : null}
          {hasReleaseTab ? (
            <TabsTrigger value="release">
              Release{releases.length ? ` (${releases.length})` : ''}
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
              keepMounted
              className={cn(usesTabbedArtifacts && 'h-full min-h-0')}
            >
              <TerminalLogViewer
                logs={mergedLogs}
                stepResults={build.step_results ?? []}
                isStreaming={isStreaming && !isTerminal}
                fillAvailableHeight={usesTabbedArtifacts}
                isLoading={isTerminal && fullLogsQuery.isLoading}
                logsUnavailable={fullLogsQuery.isError}
                onRetryLogs={() => void fullLogsQuery.refetch()}
                isTerminal={isTerminal}
              />
            </TabsContent>
            <TabsContent value="timeline">
              <EventTimeline events={events} />
            </TabsContent>
            {hasTestsTab ? (
              <TabsContent value="tests">
                <TestsPanel
                  buildId={build.id}
                  data={testsQuery.data}
                  error={testsQuery.error}
                  isLoading={testsQuery.isLoading}
                  onRetry={() => void testsQuery.refetch()}
                />
              </TabsContent>
            ) : null}
            {hasReleaseTab ? (
              <TabsContent value="release">
                <ReleasePanel
                  error={releasesQuery.error}
                  isLoading={releasesQuery.isLoading}
                  onRetry={() => void releasesQuery.refetch()}
                  releases={releases}
                  selectedReleaseId={selectedReleaseId}
                />
              </TabsContent>
            ) : null}
            {usesTabbedArtifacts ? (
              <TabsContent value="artifacts">
                <ArtifactsPanel
                  artifacts={artifacts}
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
                artifacts={artifacts}
                isLoading={artifactsQuery.isLoading}
                buildStatus={build.status}
                canManageShareLinks={canManageShareLinks}
              />
            </aside>
          ) : null}
        </div>
      </Tabs>

      {rejectOpen ? (
        <Suspense
          fallback={
            <span className="sr-only" role="status">
              Loading reject confirmation
            </span>
          }
        >
          <RejectBuildDialog
            buildNumber={build.build_number}
            isPending={rejectMutation.isPending}
            onReject={handleReject}
            onOpenChange={setRejectOpen}
            open
          />
        </Suspense>
      ) : null}
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
