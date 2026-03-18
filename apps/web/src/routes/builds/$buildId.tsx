import { createFileRoute, useNavigate } from '@tanstack/react-router'
import { useCallback, useMemo, useRef, useState } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  Copy01Icon,
  Download04Icon,
  File01Icon,
  GitBranchIcon,
  GitCommitIcon,
  InformationCircleIcon,
  Refresh01Icon,
  TimeQuarterPassIcon,
} from '@hugeicons/core-free-icons'
import { toast } from 'sonner'

import type { Artifact, BuildLogChunk } from '@/lib/types'
import {
  getActiveInstanceOrRedirect,
  requireAuthOrRedirect,
} from '@/lib/instance-context'
import { useBreadcrumbStore } from '@/stores/breadcrumb-store'
import { useBreadcrumbLabel } from '@/hooks/use-breadcrumb-label'
import { useBuildNotification } from '@/hooks/use-build-notification'
import {
  isTerminalStatus,
  useArtifactDownloadLink,
  useArtifacts,
  useBuild,
  useBuildLogs,
  useCancelBuild,
} from '@/hooks/use-builds'
import { useLogStream } from '@/hooks/use-log-stream'
import { getStatusVariant } from '@/lib/status-variants'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Skeleton } from '@/components/ui/skeleton'
import { Alert, AlertDescription } from '@/components/ui/alert'
import PageLayout from '@/components/page-layout'
import PageHeader from '@/components/page-header'
import TerminalLogViewer from '@/components/terminal-log-viewer'
import TriggerBuildDialog from '@/components/trigger-build-dialog'
import {
  formatDuration,
  formatFileSize,
  relativeTime,
} from '@/lib/format-utils'
import { PageMeta } from '@/lib/seo'

export const Route = createFileRoute('/builds/$buildId')({
  staticData: { breadcrumbLabel: 'Details' },
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireAuthOrRedirect(instance.id)
  },
  component: BuildDetailPage,
})

function artifactTypeBadgeVariant(type: Artifact['artifact_type']) {
  switch (type) {
    case 'apk':
      return 'info' as const
    case 'ipa':
      return 'success' as const
    case 'app':
      return 'warning' as const
    default:
      return 'secondary' as const
  }
}

function BuildDetailPage() {
  const { buildId } = Route.useParams()
  const navigate = useNavigate()
  const knownTerminalRef = useRef(false)
  const [rerunOpen, setRerunOpen] = useState(false)
  const buildQuery = useBuild(buildId, {
    refetchInterval: knownTerminalRef.current ? false : 3000,
  })
  const { data, isLoading, error, refetch: refetchBuild } = buildQuery
  const artifactsQuery = useArtifacts(buildId, {
    refetchInterval: knownTerminalRef.current ? false : 3000,
  })
  const { refetch: refetchArtifacts } = artifactsQuery
  const cancelMutation = useCancelBuild()

  const buildStatus = data?.build.status
  const isTerminal = buildStatus ? isTerminalStatus(buildStatus) : false

  if (isTerminal) knownTerminalRef.current = true

  const setLabel = useBreadcrumbStore((s) => s.setLabel)

  const label = data?.build.build_number
    ? `Build #${data.build.build_number}`
    : 'Build Details'

  useBreadcrumbLabel(setLabel, '/builds/$buildId', data?.build.build_number ? `Build #${data.build.build_number}` : undefined)

  // ── Build notifications (title + browser Notification) ──
  useBuildNotification(data?.build, isTerminal)

  // ── Log stream / fetch ───────────────────────────────────

  const streamEnabled = !isTerminal
  const {
    logs: streamLogs,
    isStreaming,
    error: streamError,
  } = useLogStream(buildId, streamEnabled, {
    onDone: useCallback(() => {
      void refetchBuild()
      void refetchArtifacts()
    }, [refetchBuild, refetchArtifacts]),
  })
  const { data: fullLogsData } = useBuildLogs(buildId)

  const mergedLogs: Array<BuildLogChunk> = useMemo(() => {
    if (streamEnabled && streamLogs.length > 0) return streamLogs
    if (isTerminal && fullLogsData?.logs) return fullLogsData.logs
    if (streamLogs.length > 0) return streamLogs
    return fullLogsData?.logs ?? []
  }, [streamEnabled, streamLogs, isTerminal, fullLogsData?.logs])

  // ── Handlers ─────────────────────────────────────────────

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

  // ── Loading / error states ───────────────────────────────

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

  return (
    <PageLayout width="full">
      <PageMeta title={label} noindex />
      <PageHeader
        title={`Build #${build.build_number}`}
        back={{ to: '/builds', label: 'Builds' }}
        meta={
          <>
            <Badge variant={getStatusVariant(build.status)}>
              {build.status}
            </Badge>
            <Badge variant="outline">{build.trigger_type}</Badge>
            {build.branch ? (
              <span className="inline-flex items-center gap-1 font-mono text-[11px]">
                <HugeiconsIcon icon={GitBranchIcon} size={12} />
                {build.branch}
              </span>
            ) : null}
            {build.commit_sha ? (
              <span className="inline-flex items-center gap-1 font-mono text-[11px]">
                <HugeiconsIcon icon={GitCommitIcon} size={12} />
                {build.commit_sha.slice(0, 7)}
              </span>
            ) : null}
            {duration != null ? (
              <span className="inline-flex items-center gap-1">
                <HugeiconsIcon icon={TimeQuarterPassIcon} size={12} />
                {formatDuration(duration)}
              </span>
            ) : null}
            <span className="text-border">|</span>
            <span>
              Queued {relativeTime(build.queued_at)}
              {build.started_at
                ? ` \u2192 Started ${relativeTime(build.started_at)}`
                : ''}
              {build.finished_at
                ? ` \u2192 Finished ${relativeTime(build.finished_at)}`
                : ''}
            </span>
          </>
        }
        actions={
          <div className="flex items-center gap-2">
            {isTerminal ? (
              <Button
                variant="outline"
                size="sm"
                onClick={() => setRerunOpen(true)}
              >
                <HugeiconsIcon icon={Refresh01Icon} size={14} />
                Re-run
              </Button>
            ) : null}
            {canCancel ? (
              <Button
                variant="destructive"
                size="sm"
                onClick={handleCancel}
                disabled={cancelMutation.isPending}
              >
                {cancelMutation.isPending ? 'Canceling...' : 'Cancel Build'}
              </Button>
            ) : null}
          </div>
        }
      />

      {/* Two-column layout: logs + sidebar */}
      <div className="grid gap-6 xl:grid-cols-[1fr_340px]">
        {/* Main: Terminal log viewer */}
        <div className="min-w-0">
          <TerminalLogViewer
            logs={mergedLogs}
            stepResults={build.step_results ?? []}
            isStreaming={isStreaming}
            streamError={streamError ?? undefined}
          />
        </div>

        {/* Sidebar: Artifacts + Event Timeline */}
        <aside className="space-y-4 xl:sticky xl:top-16 xl:max-h-[calc(100vh-5rem)] xl:overflow-y-auto">
          <ArtifactsPanel
            artifacts={artifactsQuery.data?.artifacts ?? []}
            isLoading={artifactsQuery.isLoading}
            buildStatus={build.status}
          />

          <EventTimeline events={events} />
        </aside>
      </div>

      {/* Re-run dialog */}
      <TriggerBuildDialog
        open={rerunOpen}
        onOpenChange={setRerunOpen}
        fixedProjectId={build.project_id}
        fixedPipelineId={build.pipeline_id}
        defaultBranch={build.branch ?? undefined}
        title="Re-run Build"
        description={`Re-run build #${build.build_number} with the same pipeline and branch.`}
        onBuildCreated={(newBuildId) => {
          void navigate({
            to: '/builds/$buildId',
            params: { buildId: newBuildId },
          })
        }}
      />
    </PageLayout>
  )
}

function ArtifactsPanel({
  artifacts,
  isLoading,
  buildStatus,
}: {
  artifacts: Array<Artifact>
  isLoading: boolean
  buildStatus: string
}) {
  const downloadMutation = useArtifactDownloadLink()

  function handleDownload(artifactId: string, name: string) {
    downloadMutation.mutate(artifactId, {
      onSuccess: (res) => {
        window.open(res.download_url, '_blank', 'noopener,noreferrer')
      },
      onError: (err) => {
        toast.error(`Failed to get download link for ${name}: ${err.message}`)
      },
    })
  }

  function handleCopyLink(artifactId: string, name: string) {
    downloadMutation.mutate(artifactId, {
      onSuccess: (res) => {
        void navigator.clipboard.writeText(res.download_url).then(
          () => toast.success(`Download link copied for ${name}`),
          () => toast.error('Failed to copy link'),
        )
      },
      onError: (err) => {
        toast.error(`Failed to get link for ${name}: ${err.message}`)
      },
    })
  }

  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle className="flex items-center gap-2 text-sm font-medium uppercase tracking-wider text-muted-foreground">
          <HugeiconsIcon icon={File01Icon} size={14} />
          Artifacts
          {artifacts.length > 0 ? (
            <Badge variant="secondary" className="text-[10px]">
              {artifacts.length}
            </Badge>
          ) : null}
        </CardTitle>
      </CardHeader>
      <CardContent>
        {isLoading ? (
          <div className="space-y-2">
            <Skeleton className="h-8 w-full" />
            <Skeleton className="h-8 w-full" />
          </div>
        ) : !artifacts.length ? (
          <p className="text-xs text-muted-foreground">
            {buildStatus === 'succeeded' || buildStatus === 'failed'
              ? 'No artifacts were produced. Check that your pipeline has artifact patterns configured.'
              : 'Artifacts will appear here once the build produces them.'}
          </p>
        ) : (
          <div className="space-y-2">
            {artifacts.map((artifact) => (
              <div
                key={artifact.id}
                className="flex items-center gap-2 border p-2"
              >
                <div className="min-w-0 flex-1">
                  <p className="truncate text-xs font-medium">
                    {artifact.name}
                  </p>
                  <div className="mt-0.5 flex items-center gap-1.5">
                    <Badge
                      variant={artifactTypeBadgeVariant(artifact.artifact_type)}
                      className="text-[10px]"
                    >
                      {artifact.artifact_type}
                    </Badge>
                    <span className="text-[10px] text-muted-foreground">
                      {artifact.file_size != null
                        ? formatFileSize(artifact.file_size)
                        : '—'}
                    </span>
                  </div>
                </div>
                <div className="flex items-center gap-0.5">
                  <Button
                    variant="ghost"
                    size="icon"
                    className="size-7 shrink-0"
                    title="Copy download link"
                    aria-label={`Copy link for ${artifact.name}`}
                    onClick={() => handleCopyLink(artifact.id, artifact.name)}
                    disabled={downloadMutation.isPending}
                  >
                    <HugeiconsIcon icon={Copy01Icon} size={14} />
                  </Button>
                  <Button
                    variant="ghost"
                    size="icon"
                    className="size-7 shrink-0"
                    title="Download"
                    aria-label={`Download ${artifact.name}`}
                    onClick={() => handleDownload(artifact.id, artifact.name)}
                    disabled={downloadMutation.isPending}
                  >
                    <HugeiconsIcon icon={Download04Icon} size={14} />
                  </Button>
                </div>
              </div>
            ))}
          </div>
        )}
      </CardContent>
    </Card>
  )
}

function EventTimeline({
  events,
}: {
  events: Array<{
    id: string
    from_status?: string
    to_status: string
    reason?: string
    actor?: string
    created_at: number
  }>
}) {
  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle className="text-sm font-medium uppercase tracking-wider text-muted-foreground">
          Event Timeline
        </CardTitle>
      </CardHeader>
      <CardContent>
        {events.length === 0 ? (
          <p className="text-xs text-muted-foreground">No events yet.</p>
        ) : (
          <div className="relative space-y-0">
            {events.map((event, i) => (
              <div
                key={event.id}
                className="relative flex gap-3 pb-4 last:pb-0"
              >
                {/* Vertical line */}
                {i < events.length - 1 ? (
                  <div className="absolute left-[5px] top-3 bottom-0 w-px bg-border" />
                ) : null}
                {/* Dot */}
                <div className="relative mt-1 size-[11px] shrink-0 rounded-full border-2 border-primary bg-background" />
                {/* Content */}
                <div className="min-w-0 flex-1">
                  <div className="flex items-baseline justify-between gap-2">
                    <p className="text-xs font-medium">
                      {event.from_status ? (
                        <span className="text-muted-foreground">
                          {event.from_status} →{' '}
                        </span>
                      ) : null}
                      {event.to_status}
                    </p>
                    <span
                      className="shrink-0 text-[10px] text-muted-foreground"
                      title={new Date(event.created_at * 1000).toLocaleString()}
                    >
                      {relativeTime(event.created_at)}
                    </span>
                  </div>
                  {event.reason ? (
                    <p className="mt-0.5 text-[11px] text-muted-foreground">
                      {event.reason}
                    </p>
                  ) : null}
                </div>
              </div>
            ))}
          </div>
        )}
      </CardContent>
    </Card>
  )
}
