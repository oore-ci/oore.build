import { lazy, Suspense, useMemo, useState } from 'react'
import { Link, useNavigate } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  ArrowDown01Icon,
  CommandLineIcon,
  Copy01Icon,
  Download04Icon,
  InformationCircleIcon,
  Notification03Icon,
  Refresh01Icon,
  StopCircleIcon,
} from '@hugeicons/core-free-icons'
import { detectPlatform } from '@tanstack/react-hotkeys'
import { toast } from '@/lib/toast'

import { ArtifactsPanel } from './artifacts-panel'
import { BuildStateAlert } from './build-state-alert'
import { buildStateSummary } from '@/lib/build-state-description'
import {
  artifactInstallReadiness,
  latestAppPerPlatform,
} from '@/lib/artifact-install'
import { SHORTCUTS, shortcutHint } from '@/lib/keyboard-shortcuts'
import { useAppShortcut } from '@/hooks/use-app-shortcut'
import { useCopyInstallLink } from '@/hooks/use-copy-install-link'
import { useRerunAndOpen } from '@/hooks/use-rerun-and-open'
import { usePageCommands, type PageCommand } from '@/stores/command-store'
import { retryStepLabel } from '@/lib/build-steps'
import { BuildSummary, buildSourceRef } from './build-summary'
import { EventTimeline } from './event-timeline'
import { AnalyzerPanel } from './analyzer-panel'
import { TestsPanel } from './tests-panel'
import type {
  BuildFeedback,
  BuildLogChunk,
  StepResult,
} from '@oore/client/models'
import {
  useBuildNotification,
  useNotificationPermission,
} from '@/hooks/use-build-notification'
import {
  isTerminalStatus,
  useArtifacts,
  useBuild,
  useBuilds,
  useBuildApprovalDecision,
  useBuildLogs,
  useBuildSizeReport,
  useBuildAnalysis,
  useBuildTests,
  useCancelBuild,
  useRetryBuildStep,
} from '@/hooks/use-builds'
import { useLogStream } from '@/hooks/use-log-stream'
import { useBuildFeedback } from '@/hooks/use-build-feedback'
import { useReleases } from '@/hooks/use-releases'
import { ReleasePanel } from '@/components/releases/release-panel'
import {
  hasProjectPermission,
  useHasPermissions,
} from '@/hooks/use-permissions'
import { useProject } from '@/hooks/use-projects'
import { mergeBuildLogSnapshots } from '@/lib/log-stream-utils'
import { stepMedians } from '@/lib/step-timings'
import { isOoreApiError } from '@oore/client/client'
import { PageMeta } from '@/lib/seo'
import { TRIGGER_TYPE_LABELS } from '@/lib/status-variants'
import { formatEnumLabel, relativeTime } from '@/lib/format-utils'
import { StatusBadge } from '@/components/status-badge'
import PageHeader from '@/components/page-header'
import { ResumeRunnersButton } from '@/components/settings/direct-runner-policy-panel'
import PageLayout from '@/components/page-layout'
import { NotFoundState } from '@/components/root-route-boundaries'
import TerminalLogViewer from '@/components/terminal-log-viewer'
import {
  currentStepName,
  findFailureLine,
  groupLogs,
} from '@/components/terminal-log-viewer/log-model'
import type { LogFocusRequest } from '@/components/terminal-log-viewer/types'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyTitle,
} from '@/components/ui/empty'
import { Skeleton } from '@/components/ui/skeleton'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { useTime } from '@/hooks/use-time'
import { LIVE_FALLBACK_POLL_MS } from '@/lib/live-events'
import { useLiveEventsStore } from '@/stores/live-events-store'
import { Row } from '@/components/layout/row'
import { SplitRow } from '@/components/layout/split-row'
import { Text } from '@/components/layout/text'
import { splitRowVariants, textVariants } from '@/lib/recipes'
import { analyzerTabCount } from '@/lib/analyzer-format'
import { cn } from '@/lib/utils'

const loadCancelBuildDialog = () => import('./cancel-build-dialog')
const CancelBuildDialog = lazy(loadCancelBuildDialog)
const loadRejectBuildDialog = () => import('./reject-build-dialog')
const RejectBuildDialog = lazy(loadRejectBuildDialog)

const NO_STEP_RESULTS: Array<StepResult> = []
const NO_FEEDBACK: Array<BuildFeedback> = []

function missingTabMessage(
  tab: string | number,
  shownTab: string | number | null,
) {
  const reason =
    tab === 'tests'
      ? 'This build has no test results'
      : tab === 'analyzer'
        ? 'This build has no analyzer results'
        : tab === 'release'
          ? 'This build has not been sent to a store'
          : `This build has no “${tab}” view`
  return `${reason}, so ${shownTab === 'tests' ? 'Tests' : 'Logs'} is shown instead.`
}

export function BuildDetailPage({
  buildId,
  initialTab,
  selectedReleaseId,
}: {
  buildId: string
  initialTab?: string
  selectedReleaseId?: string
}) {
  const navigate = useNavigate()
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
  const rerunAndOpen = useRerunAndOpen()
  const copyInstallLink = useCopyInstallLink()
  const retryStepMutation = useRetryBuildStep()
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
  // Unfinished builds tick every second for their duration; finished builds
  // only refresh relative times once a minute.
  const nowSeconds = useTime((now) =>
    isTerminal ? Math.floor(now / 60_000) * 60 : Math.floor(now / 1000),
  )
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
  const [chosenTab, setActiveTab] = useState<string | number | null>(null)
  const [logFocus, setLogFocus] = useState<LogFocusRequest | null>(null)
  const feedbackQuery = useBuildFeedback(buildId)
  const feedback = feedbackQuery.data?.feedback ?? NO_FEEDBACK
  const sizesQuery = useBuildSizeReport(buildId, {
    refetchInterval: isTerminal ? false : 5000,
  })
  const testsQuery = useBuildTests(buildId, {
    refetchInterval: isTerminal ? false : 5000,
  })
  const analysisQuery = useBuildAnalysis(buildId, {
    refetchInterval: isTerminal ? false : 5000,
  })
  const analysisCounts = analysisQuery.data?.analysis?.counts
  const hasAnalyzerTab =
    !!analysisCounts ||
    analysisQuery.isError ||
    (initialTab === 'analyzer' && analysisQuery.isLoading)
  const testSummary = testsQuery.data?.summary
  const hasTestsTab =
    !!testSummary ||
    testsQuery.isError ||
    (initialTab === 'tests' && testsQuery.isLoading)
  // The daemon records a release while the build is still running, so a build
  // that failed after publishing still has a live rollout to watch or halt.
  const releasesQuery = useReleases(
    { build_id: buildId },
    { enabled: !!buildStatus, pollWhenIdle: !!buildStatus && !isTerminal },
  )
  const releases = releasesQuery.data?.releases ?? []
  const hasReleaseTab =
    releases.length > 0 ||
    releasesQuery.isError ||
    (initialTab === 'release' && releasesQuery.isLoading)
  const availableTabs = new Set<string | number>([
    'logs',
    'timeline',
    'artifacts',
  ])
  if (hasTestsTab) availableTabs.add('tests')
  if (hasAnalyzerTab) availableTabs.add('analyzer')
  if (hasReleaseTab) availableTabs.add('release')
  const requestedTab = chosenTab ?? initialTab ?? null
  // A finished build with failing tests opens on Tests until the user picks a tab.
  // Decide once the build and its test results have loaded, then keep it, so
  // the page never jumps from Logs to Tests under the user.
  const settledDefaultTab = !buildStatus
    ? null
    : !isTerminal
      ? 'logs'
      : testsQuery.isPending
        ? null
        : testSummary?.failed
          ? 'tests'
          : 'logs'
  const [defaultTab, setDefaultTab] = useState<string | null>(null)
  if (defaultTab === null && settledDefaultTab !== null) {
    setDefaultTab(settledDefaultTab)
  }
  const activeTab =
    requestedTab !== null && availableTabs.has(requestedTab)
      ? requestedTab
      : (defaultTab ?? settledDefaultTab)
  const missingTab =
    chosenTab === null && initialTab && !availableTabs.has(initialTab)
      ? initialTab
      : null

  const label = data?.build.build_number
    ? `Build #${data.build.build_number}`
    : 'Build Details'

  useBuildNotification(data?.build, isTerminal)
  const notifications = useNotificationPermission()

  const streamEnabled = !isTerminal
  const {
    logs: streamLogs,
    isStreaming,
    isPolling,
    reconnect,
  } = useLogStream(buildId, streamEnabled, {
    onDone: () => {
      void refetchBuild()
      void refetchArtifacts()
    },
  })
  const fullLogsQuery = useBuildLogs(buildId, { enabled: isTerminal })
  const { data: fullLogsData } = fullLogsQuery

  const mergedLogs: Array<BuildLogChunk> = useMemo(() => {
    return mergeBuildLogSnapshots(streamLogs, fullLogsData?.logs ?? [])
  }, [streamLogs, fullLogsData?.logs])
  const stepResults = data?.build.step_results
  // Recent successful runs of this pipeline give each step a typical time.
  const pipelineHistoryQuery = useBuilds(
    {
      pipeline_id: data?.build.pipeline_id,
      status: 'succeeded',
      sort: 'created_at',
      direction: 'desc',
      limit: 20,
    },
    { enabled: !!data?.build.step_results?.length },
  )
  const medians = useMemo(
    () => stepMedians(pipelineHistoryQuery.data?.builds ?? [], buildId),
    [pipelineHistoryQuery.data?.builds, buildId],
  )
  const failureLine = useMemo(() => {
    if (buildStatus !== 'failed' && buildStatus !== 'timed_out') return null
    const { stepGroups, allVisibleLogs } = groupLogs(
      mergedLogs,
      stepResults ?? [],
    )
    return findFailureLine(stepGroups, allVisibleLogs)
  }, [buildStatus, mergedLogs, stepResults])
  const runningStep = useMemo(
    () => (buildStatus === 'running' ? currentStepName(mergedLogs) : null),
    [buildStatus, mergedLogs],
  )

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
        toast.success('Build stopped')
        setCancelOpen(false)
      },
      onError: (err) => {
        toast.error(`Could not stop the build: ${err.message}`)
      },
    })
  }

  // Worked out before the loading states return, so the shortcuts and
  // Quick open actions below register on every render.
  const loadedBuild = data?.build
  const canDecideApproval =
    loadedBuild?.approval?.state === 'pending' && canApproveBuild
  const canRerun = !!loadedBuild && isTerminal && canTriggerBuild
  // A maintainer rejects rather than cancels a build waiting for approval.
  const canCancel =
    !!loadedBuild && !isTerminal && canCancelBuild && !canDecideApproval
  // A build no runner has started has no log yet; skip the empty workbench.
  const showLogWorkbench = !!loadedBuild?.started_at || mergedLogs.length > 0
  const installApps =
    loadedBuild && isTerminal ? latestAppPerPlatform(artifacts) : []
  const platform = detectPlatform()

  function rerun() {
    if (loadedBuild && canRerun) rerunAndOpen.rerun(loadedBuild.id)
  }

  function requestCancel() {
    if (canCancel && !cancelMutation.isPending) setCancelOpen(true)
  }

  function focusLogs() {
    setActiveTab('logs')
    requestAnimationFrame(() => {
      document
        .querySelector<HTMLElement>(
          '[data-build-log-output] [data-slot="scroll-area-viewport"]',
        )
        ?.focus()
    })
  }

  useAppShortcut(SHORTCUTS.rerun, rerun, canRerun)
  useAppShortcut(SHORTCUTS.cancel, requestCancel, canCancel)
  useAppShortcut(SHORTCUTS.focusLogs, focusLogs, showLogWorkbench)

  const buildNumberLabel = loadedBuild ? `#${loadedBuild.build_number}` : ''
  const pageCommands: Array<PageCommand> = [
    ...(canRerun && loadedBuild
      ? [
          {
            id: `rerun-${loadedBuild.id}`,
            label: `Re-run build ${buildNumberLabel}`,
            icon: Refresh01Icon,
            hint: shortcutHint(SHORTCUTS.rerun, platform),
            keywords: ['rerun retry again'],
            run: rerun,
          },
        ]
      : []),
    ...(canCancel && loadedBuild
      ? [
          {
            id: `cancel-${loadedBuild.id}`,
            label: `Stop build ${buildNumberLabel}…`,
            icon: StopCircleIcon,
            hint: shortcutHint(SHORTCUTS.cancel, platform),
            keywords: ['cancel stop abort'],
            run: requestCancel,
          },
        ]
      : []),
    ...installApps.map(({ artifact, label }) => ({
      id: `install-${artifact.id}`,
      label:
        installApps.length === 1
          ? 'Copy install link'
          : `Copy ${label} install link`,
      icon: Copy01Icon,
      keywords: ['share install link url'],
      run: () => {
        if (loadedBuild) copyInstallLink(loadedBuild.id, artifact, label)
      },
    })),
    ...(showLogWorkbench
      ? [
          {
            id: 'focus-logs',
            label: 'Focus logs',
            icon: CommandLineIcon,
            hint: shortcutHint(SHORTCUTS.focusLogs, platform),
            keywords: ['log output scroll'],
            run: focusLogs,
          },
        ]
      : []),
  ]
  usePageCommands(
    loadedBuild ? `Build ${buildNumberLabel}` : 'Build',
    pageCommands,
  )

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
    if (isOoreApiError(error) && error.status === 404) {
      return (
        <PageLayout width="full">
          <PageMeta title="Build not found" noindex />
          <NotFoundState
            title="Build not found"
            description="This build was deleted, expired, or the link is wrong."
            backTo="/builds"
            backLabel="Back to builds"
          />
        </PageLayout>
      )
    }

    return (
      <PageLayout width="full">
        <PageMeta title={label} noindex />
        <Alert variant="destructive">
          <HugeiconsIcon icon={InformationCircleIcon} size={16} />
          <AlertDescription className={splitRowVariants()}>
            <span>Failed to load build: {error.message}</span>
            <span className="flex gap-2">
              <Button
                variant="outline"
                size="sm"
                render={<Link to="/builds" />}
                nativeButton={false}
              >
                Back to builds
              </Button>
              <Button
                variant="outline"
                size="sm"
                onClick={() => void refetchBuild()}
              >
                Retry
              </Button>
            </span>
          </AlertDescription>
        </Alert>
      </PageLayout>
    )
  }

  if (!data) return null

  const { build, events } = data
  const retryableSteps = canTriggerBuild ? (data.retryable_steps ?? []) : []
  const firstRetryLabel = retryableSteps[0]
    ? retryStepLabel(retryableSteps[0])
    : null
  const awaitingApproval = build.approval?.state === 'pending'
  const duration = build.started_at
    ? (build.finished_at ?? nowSeconds) - build.started_at
    : null
  const stateSummary = buildStateSummary(build, events, {
    canApprove: canDecideApproval,
  })
  const sourceRef = buildSourceRef(build)
  const approvalDetails = awaitingApproval
    ? [
        {
          label: 'Trigger',
          value: `${TRIGGER_TYPE_LABELS[build.trigger_type]}${build.trigger_event ? ` · ${formatEnumLabel(build.trigger_event).toLowerCase()}` : ''}`,
        },
        ...(build.trigger_actor
          ? [{ label: 'Requested by', value: build.trigger_actor }]
          : []),
        ...(sourceRef
          ? [
              {
                label: build.branch ? 'Branch' : 'Ref',
                value: <span className="font-mono text-xs">{sourceRef}</span>,
              },
            ]
          : []),
        ...(build.commit_sha
          ? [
              {
                label: 'Commit',
                value: (
                  <span className="font-mono text-xs">
                    {build.commit_sha.slice(0, 7)}
                  </span>
                ),
              },
            ]
          : []),
        {
          label: 'Waiting since',
          value: relativeTime(build.approval?.requested_at ?? build.queued_at),
        },
      ]
    : undefined
  const installableApps =
    build.status === 'succeeded'
      ? artifacts.filter((artifact) => artifactInstallReadiness(artifact).ready)
      : []
  const showTests = () => setActiveTab('tests')
  const nextStep =
    build.status === 'failed' || build.status === 'timed_out' ? (
      firstRetryLabel ? (
        `The build's artifacts are stored, so once the cause is fixed you can retry the ${firstRetryLabel} without rebuilding.`
      ) : testSummary?.failed ? (
        <>
          {testSummary.failed === 1
            ? '1 test failed.'
            : `${testSummary.failed} tests failed.`}{' '}
          <Button
            variant="link"
            size="xs"
            className={cn(
              textVariants({ variant: 'muted' }),
              'h-auto px-0 align-baseline underline',
            )}
            onClick={showTests}
          >
            Open Tests
          </Button>
        </>
      ) : (
        'Fix the cause, then re-run the build.'
      )
    ) : (build.status === 'canceled' || build.status === 'expired') &&
      canTriggerBuild ? (
      'Re-run it to start a new build.'
    ) : null

  return (
    <PageLayout
      width="full"
      // The log panel takes the height that is left and scrolls inside, so
      // the steps and toolbar stay in view. Other tabs scroll the page.
      className={
        activeTab === 'logs' && showLogWorkbench ? 'min-h-0 flex-1' : 'flex-1'
      }
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
            <div className="mr-2 flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1">
              <StatusBadge
                kind="build"
                value={build.status}
                className="h-6 px-2.5 text-[13px]"
              />
              {build.approval ? (
                <StatusBadge kind="approval" value={build.approval.state} />
              ) : null}
              {runningStep ? (
                <span className="min-w-0 truncate text-sm text-foreground">
                  {runningStep}
                </span>
              ) : null}
              <span>
                {TRIGGER_TYPE_LABELS[build.trigger_type]}
                {build.trigger_event
                  ? ` · ${formatEnumLabel(build.trigger_event).toLowerCase()}`
                  : ''}
              </span>
            </div>
            <BuildSummary build={build} duration={duration} />
          </>
        }
        actions={
          <Row gap={2} className="flex-wrap justify-end">
            {!isTerminal && notifications.permission === 'default' ? (
              <Button variant="ghost" size="sm" onClick={notifications.request}>
                <HugeiconsIcon icon={Notification03Icon} size={14} />
                Notify me when done
              </Button>
            ) : null}
            {installableApps.length === 1 ? (
              <Button
                size="sm"
                render={
                  <Link
                    to="/builds/$buildId"
                    params={{ buildId: build.id }}
                    search={{ install: installableApps[0].id }}
                  />
                }
                nativeButton={false}
              >
                <HugeiconsIcon icon={Download04Icon} size={14} />
                Install
              </Button>
            ) : installableApps.length > 1 ? (
              <DropdownMenu>
                <DropdownMenuTrigger render={<Button size="sm" />}>
                  <HugeiconsIcon icon={Download04Icon} size={14} />
                  Install
                  <HugeiconsIcon
                    icon={ArrowDown01Icon}
                    size={14}
                    data-icon="inline-end"
                  />
                </DropdownMenuTrigger>
                <DropdownMenuContent align="end" className="w-auto">
                  <DropdownMenuGroup>
                    <DropdownMenuLabel>Apps to install</DropdownMenuLabel>
                    {installableApps.map((artifact) => (
                      <DropdownMenuItem
                        key={artifact.id}
                        render={
                          <Link
                            to="/builds/$buildId"
                            params={{ buildId: build.id }}
                            search={{ install: artifact.id }}
                          />
                        }
                      >
                        <span className="text-muted-foreground">
                          {artifact.artifact_type === 'apk' ? 'Android' : 'iOS'}
                        </span>
                        {artifact.name}
                      </DropdownMenuItem>
                    ))}
                  </DropdownMenuGroup>
                </DropdownMenuContent>
              </DropdownMenu>
            ) : null}
            {retryableSteps.map((step) => (
              <Button
                key={step}
                size="sm"
                variant={build.status === 'failed' ? 'default' : 'outline'}
                onClick={() => {
                  retryStepMutation.mutate(
                    { buildId: build.id, step },
                    {
                      onSuccess: (result) => {
                        toast.success(
                          `Retrying the ${retryStepLabel(step)} as build #${result.build.build_number}`,
                        )
                        void navigate({
                          to: '/builds/$buildId',
                          params: { buildId: result.build.id },
                        })
                      },
                      onError: (err) => {
                        toast.error(`Failed to retry: ${err.message}`)
                      },
                    },
                  )
                }}
                disabled={retryStepMutation.isPending}
              >
                <HugeiconsIcon icon={Refresh01Icon} size={14} />
                {retryStepMutation.isPending &&
                retryStepMutation.variables.step === step
                  ? 'Retrying...'
                  : `Retry ${retryStepLabel(step)}`}
              </Button>
            ))}
            {canRerun ? (
              <Button
                variant="outline"
                size="sm"
                onClick={rerun}
                disabled={rerunAndOpen.isPending}
              >
                <HugeiconsIcon icon={Refresh01Icon} size={14} />
                {rerunAndOpen.isPending ? 'Re-running...' : 'Re-run'}
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
                variant="outline"
                size="sm"
                onClick={() => setCancelOpen(true)}
                disabled={cancelMutation.isPending}
              >
                {cancelMutation.isPending ? 'Stopping...' : 'Stop build'}
              </Button>
            ) : null}
          </Row>
        }
      />

      {build.runner_policy_block_reason ? (
        <Alert>
          <HugeiconsIcon icon={InformationCircleIcon} size={16} />
          <AlertDescription>
            {build.runner_policy_block_reason === 'instance_paused' ? (
              canWriteInstanceSettings ? (
                <SplitRow>
                  <span>
                    This build is waiting because runners are paused on{' '}
                    <Link
                      to="/settings/runners"
                      className="font-medium underline underline-offset-4"
                    >
                      Runners
                    </Link>
                    .
                  </span>
                  <ResumeRunnersButton />
                </SplitRow>
              ) : (
                <>
                  This build is waiting because runners are paused. Ask an owner
                  or admin to resume them.
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

      {stateSummary ? (
        <BuildStateAlert
          summary={stateSummary}
          failureLine={failureLine}
          nextStep={nextStep}
          details={approvalDetails}
          onShowFailureLine={() => {
            if (!failureLine) return
            setActiveTab('logs')
            setLogFocus({
              step: failureLine.step,
              sequence: failureLine.sequence,
              key: Date.now(),
            })
          }}
        />
      ) : null}

      <Tabs
        value={activeTab}
        onValueChange={setActiveTab}
        className="min-h-0 flex-1 gap-3"
      >
        <TabsList variant="line" aria-label="Build details">
          <TabsTrigger value="logs">Logs</TabsTrigger>
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
          {hasAnalyzerTab ? (
            <TabsTrigger value="analyzer">
              Analyzer
              {analysisCounts && analyzerTabCount(analysisCounts)
                ? ` (${analyzerTabCount(analysisCounts)})`
                : ''}
            </TabsTrigger>
          ) : null}
          <TabsTrigger value="artifacts">
            Artifacts{artifacts.length ? ` (${artifacts.length})` : ''}
          </TabsTrigger>
          {hasReleaseTab ? (
            <TabsTrigger value="release">
              Release{releases.length ? ` (${releases.length})` : ''}
            </TabsTrigger>
          ) : null}
          <TabsTrigger value="timeline">
            Timeline
            {events.length + feedback.length > 0
              ? ` (${events.length + feedback.length})`
              : ''}
          </TabsTrigger>
        </TabsList>
        {missingTab ? (
          <Text variant="muted" role="status">
            {missingTabMessage(missingTab, activeTab)}
          </Text>
        ) : null}
        <TabsContent value="logs" keepMounted className="flex min-h-0 flex-col">
          {showLogWorkbench ? (
            <TerminalLogViewer
              logs={mergedLogs}
              stepResults={build.step_results ?? NO_STEP_RESULTS}
              stepMedians={medians}
              isStreaming={isStreaming && !isTerminal}
              fillAvailableHeight
              isLoading={isTerminal && fullLogsQuery.isLoading}
              logsUnavailable={fullLogsQuery.isError}
              onRetryLogs={() => void fullLogsQuery.refetch()}
              isTerminal={isTerminal}
              buildStatus={build.status}
              buildNumber={build.build_number}
              isPolling={isPolling}
              onReconnect={reconnect}
              focusRequest={logFocus}
            />
          ) : (
            <Empty className="flex-none border">
              <EmptyHeader>
                <EmptyTitle>No logs yet</EmptyTitle>
                <EmptyDescription>
                  {build.status === 'expired'
                    ? 'This build expired before a runner started it, so it has no logs.'
                    : build.status === 'canceled'
                      ? 'This build was canceled before a runner started it, so it has no logs.'
                      : awaitingApproval
                        ? 'Logs appear here once the build is approved and a runner starts it.'
                        : 'Logs appear here once a runner starts the build.'}
                </EmptyDescription>
              </EmptyHeader>
            </Empty>
          )}
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
        {hasAnalyzerTab ? (
          <TabsContent value="analyzer">
            <AnalyzerPanel
              data={analysisQuery.data}
              error={analysisQuery.error}
              isLoading={analysisQuery.isLoading}
              onRetry={() => void analysisQuery.refetch()}
            />
          </TabsContent>
        ) : null}
        <TabsContent value="artifacts">
          <ArtifactsPanel
            artifacts={artifacts}
            error={artifactsQuery.error}
            isLoading={artifactsQuery.isLoading}
            sizes={sizesQuery.data}
            onRetry={() => void refetchArtifacts()}
            buildStatus={build.status}
            canManageShareLinks={canManageShareLinks}
          />
        </TabsContent>
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
        <TabsContent value="timeline">
          <EventTimeline events={events} feedback={feedback} />
        </TabsContent>
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
