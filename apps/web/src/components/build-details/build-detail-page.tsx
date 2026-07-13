import { useCallback, useMemo } from 'react'
import { useNavigate } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  InformationCircleIcon,
  Refresh01Icon,
} from '@hugeicons/core-free-icons'
import { toast } from 'sonner'

import { ArtifactsPanel } from './artifacts-panel'
import { BuildSummary } from './build-summary'
import { EventTimeline } from './event-timeline'
import type { BuildLogChunk } from '@/lib/types'
import { useBreadcrumbLabel } from '@/hooks/use-breadcrumb-label'
import { useBuildNotification } from '@/hooks/use-build-notification'
import {
  isTerminalStatus,
  useArtifacts,
  useBuild,
  useBuildLogs,
  useCancelBuild,
  useRerunBuild,
} from '@/hooks/use-builds'
import { useLogStream } from '@/hooks/use-log-stream'
import { READ_ONLY_REASON, isDemoMode } from '@/lib/demo-mode'
import { mergeBuildLogSnapshots } from '@/lib/log-stream-utils'
import { PageMeta } from '@/lib/seo'
import { getStatusVariant } from '@/lib/status-variants'
import { useBreadcrumbStore } from '@/stores/breadcrumb-store'
import PageHeader from '@/components/page-header'
import PageLayout from '@/components/page-layout'
import TerminalLogViewer from '@/components/terminal-log-viewer'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Skeleton } from '@/components/ui/skeleton'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'

export function BuildDetailPage({ buildId }: { buildId: string }) {
  const navigate = useNavigate()
  const rerunMutation = useRerunBuild()
  const buildQuery = useBuild(buildId, {
    refetchInterval: (query) =>
      query.state.data && isTerminalStatus(query.state.data.build.status)
        ? false
        : 3000,
  })
  const { data, isLoading, error, refetch: refetchBuild } = buildQuery
  const buildStatus = data?.build.status
  const isTerminal = buildStatus ? isTerminalStatus(buildStatus) : false
  const artifactsQuery = useArtifacts(buildId, {
    refetchInterval: isTerminal ? false : 3000,
  })
  const { refetch: refetchArtifacts } = artifactsQuery
  const cancelMutation = useCancelBuild()

  const setLabel = useBreadcrumbStore((state) => state.setLabel)

  const label = data?.build.build_number
    ? `Build #${data.build.build_number}`
    : 'Build Details'

  useBreadcrumbLabel(
    setLabel,
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
    return (
      <PageLayout width="full">
        <PageMeta title={label} noindex />
        <Alert variant="destructive">
          <HugeiconsIcon icon={InformationCircleIcon} size={16} />
          <AlertDescription>
            Failed to load build: {error.message}
          </AlertDescription>
        </Alert>
      </PageLayout>
    )
  }

  if (!data) return null

  const { build, events } = data
  const canCancel = !isTerminal
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
    <PageLayout width="full">
      <PageMeta title={label} noindex />
      <PageHeader
        title={`Build #${build.build_number}`}
        description={
          [build.context?.project_name, build.context?.pipeline_name]
            .filter(Boolean)
            .join(' · ') || undefined
        }
        back={{ to: '/builds', label: 'Builds' }}
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
            {isTerminal ? (
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
                disabled={rerunMutation.isPending || isDemoMode}
                title={isDemoMode ? READ_ONLY_REASON : undefined}
              >
                <HugeiconsIcon icon={Refresh01Icon} size={14} />
                {rerunMutation.isPending ? 'Re-running...' : 'Re-run'}
              </Button>
            ) : null}
            {canCancel ? (
              <Button
                variant="destructive"
                size="sm"
                onClick={handleCancel}
                disabled={cancelMutation.isPending || isDemoMode}
                title={isDemoMode ? READ_ONLY_REASON : undefined}
              >
                {cancelMutation.isPending ? 'Canceling...' : 'Cancel Build'}
              </Button>
            ) : null}
          </div>
        }
      />

      {failureReason ? (
        <Alert variant="destructive">
          <HugeiconsIcon icon={InformationCircleIcon} size={16} />
          <AlertDescription>{failureReason}</AlertDescription>
        </Alert>
      ) : null}

      <Tabs defaultValue="logs" className="gap-3">
        <TabsList variant="line">
          <TabsTrigger value="logs">Logs</TabsTrigger>
          <TabsTrigger value="timeline">
            Timeline{events.length > 0 ? ` (${events.length})` : ''}
          </TabsTrigger>
        </TabsList>
        <div className="grid min-w-0 items-start gap-6 xl:grid-cols-[minmax(0,1fr)_22rem]">
          <div className="order-2 min-w-0 xl:order-1">
            <TabsContent value="logs">
              <TerminalLogViewer
                logs={mergedLogs}
                stepResults={build.step_results ?? []}
                isStreaming={isStreaming && !isTerminal}
                isLoading={isTerminal && fullLogsQuery.isLoading}
                logsUnavailable={fullLogsQuery.isError}
                isTerminal={isTerminal}
              />
            </TabsContent>
            <TabsContent value="timeline">
              <EventTimeline events={events} />
            </TabsContent>
          </div>
          <aside
            aria-label="Build output"
            className="order-1 xl:order-2 xl:sticky xl:top-6"
          >
            <ArtifactsPanel
              artifacts={artifactsQuery.data?.artifacts ?? []}
              isLoading={artifactsQuery.isLoading}
              buildStatus={build.status}
            />
          </aside>
        </div>
      </Tabs>
    </PageLayout>
  )
}
