import { Link, createFileRoute, useNavigate } from '@tanstack/react-router'
import { useCallback, useMemo, useState } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  Copy01Icon,
  Download04Icon,
  File01Icon,
  GitBranchIcon,
  GitCommitIcon,
  InformationCircleIcon,
  Refresh01Icon,
  Share08Icon,
  TimeQuarterPassIcon,
} from '@hugeicons/core-free-icons'
import { toast } from 'sonner'

import type {
  Artifact,
  Build,
  BuildLogChunk,
  CreateScopedDownloadTokenResponse,
} from '@/lib/types'
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
  useCreateScopedDownloadToken,
  useRerunBuild,
} from '@/hooks/use-builds'
import { useLogStream } from '@/hooks/use-log-stream'
import { getStatusVariant } from '@/lib/status-variants'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Skeleton } from '@/components/ui/skeleton'
import { Alert, AlertDescription } from '@/components/ui/alert'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Checkbox } from '@/components/ui/checkbox'
import { Label } from '@/components/ui/label'
import { Spinner } from '@/components/ui/spinner'
import PageLayout from '@/components/page-layout'
import PageHeader from '@/components/page-header'
import TerminalLogViewer from '@/components/terminal-log-viewer'
import {
  formatDuration,
  formatFileSize,
  relativeTime,
} from '@/lib/format-utils'
import { PageMeta } from '@/lib/seo'
import { READ_ONLY_REASON, isDemoMode } from '@/lib/demo-mode'
import { mergeBuildLogSnapshots } from '@/lib/log-stream-utils'

export const Route = createFileRoute('/builds/$buildId')({
  staticData: { breadcrumbLabel: 'Details' },
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireAuthOrRedirect(instance.id)
  },
  component: BuildDetailPageWrapper,
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

function BuildSummary({
  build,
  duration,
}: {
  build: Build
  duration: number | null
}) {
  const hasContext =
    build.context?.project_name ||
    build.context?.pipeline_name ||
    build.context?.runner_name ||
    build.source_build_id

  return (
    <Card size="sm" aria-label="Build summary">
      <CardContent>
        <dl className="grid gap-x-6 gap-y-4 sm:grid-cols-2 xl:grid-cols-4">
          <div className="min-w-0 space-y-1">
            <dt className="text-xs font-medium text-muted-foreground">
              Source
            </dt>
            <dd className="flex min-w-0 items-center gap-2">
              {build.branch ? (
                <span className="inline-flex min-w-0 items-center gap-1.5 font-mono text-xs">
                  <HugeiconsIcon icon={GitBranchIcon} />
                  <span className="truncate">{build.branch}</span>
                </span>
              ) : (
                <span className="text-muted-foreground">Not recorded</span>
              )}
              {build.commit_sha ? (
                <span className="inline-flex shrink-0 items-center gap-1 font-mono text-xs text-muted-foreground">
                  <HugeiconsIcon icon={GitCommitIcon} />
                  {build.commit_sha.slice(0, 7)}
                </span>
              ) : null}
            </dd>
          </div>

          <div className="space-y-1">
            <dt className="text-xs font-medium text-muted-foreground">
              Duration
            </dt>
            <dd className="inline-flex items-center gap-1.5">
              <HugeiconsIcon icon={TimeQuarterPassIcon} />
              {duration != null ? formatDuration(duration) : 'Not started'}
            </dd>
          </div>

          <div className="space-y-1">
            <dt className="text-xs font-medium text-muted-foreground">
              Timeline
            </dt>
            <dd className="text-xs leading-5">
              <span>Queued {relativeTime(build.queued_at)}</span>
              {build.started_at ? (
                <span className="block text-muted-foreground">
                  Started {relativeTime(build.started_at)}
                </span>
              ) : null}
              {build.finished_at ? (
                <span className="block text-muted-foreground">
                  Finished {relativeTime(build.finished_at)}
                </span>
              ) : null}
            </dd>
          </div>

          <div className="space-y-1">
            <dt className="text-xs font-medium text-muted-foreground">
              Context
            </dt>
            <dd className="text-xs leading-5">
              {build.context?.project_name ? (
                <span className="block">{build.context.project_name}</span>
              ) : null}
              {build.context?.pipeline_name ? (
                <span className="block text-muted-foreground">
                  {build.context.pipeline_name}
                </span>
              ) : null}
              {build.context?.runner_name ? (
                <span className="block text-muted-foreground">
                  Runner: {build.context.runner_name}
                </span>
              ) : null}
              {build.source_build_id ? (
                <Link
                  to="/builds/$buildId"
                  params={{ buildId: build.source_build_id }}
                  className="text-muted-foreground underline underline-offset-4 hover:text-foreground"
                >
                  Re-run of a previous build
                </Link>
              ) : null}
              {!hasContext ? (
                <span className="text-muted-foreground">Not recorded</span>
              ) : null}
            </dd>
          </div>
        </dl>
      </CardContent>
    </Card>
  )
}

function BuildDetailPageWrapper() {
  const { buildId } = Route.useParams()
  return <BuildDetailPage key={buildId} />
}

function BuildDetailPage() {
  const { buildId } = Route.useParams()
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

  const setLabel = useBreadcrumbStore((s) => s.setLabel)

  const label = data?.build.build_number
    ? `Build #${data.build.build_number}`
    : 'Build Details'

  useBreadcrumbLabel(
    setLabel,
    '/builds/$buildId',
    data?.build.build_number ? `Build #${data.build.build_number}` : undefined,
  )

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
  const fullLogsQuery = useBuildLogs(buildId, { enabled: isTerminal })
  const { data: fullLogsData } = fullLogsQuery

  const mergedLogs: Array<BuildLogChunk> = useMemo(() => {
    return mergeBuildLogSnapshots(streamLogs, fullLogsData?.logs ?? [])
  }, [streamLogs, fullLogsData?.logs])

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
        back={{ to: '/builds', label: 'Builds' }}
        meta={
          <>
            <Badge variant={getStatusVariant(build.status)}>
              {build.status}
            </Badge>
            <Badge variant="outline">{build.trigger_type}</Badge>
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

      <BuildSummary build={build} duration={duration} />

      {failureReason ? (
        <Alert variant="destructive">
          <HugeiconsIcon icon={InformationCircleIcon} size={16} />
          <AlertDescription>{failureReason}</AlertDescription>
        </Alert>
      ) : null}

      <section
        aria-labelledby="build-logs-heading"
        className="min-w-0 space-y-2"
      >
        <div className="flex items-center justify-between gap-3">
          <div>
            <h2 id="build-logs-heading" className="text-sm font-medium">
              Build logs
            </h2>
            <p className="text-xs text-muted-foreground">
              Live output, errors, and step-level context.
            </p>
          </div>
          {isStreaming ? <Badge variant="info">Live</Badge> : null}
        </div>
        <TerminalLogViewer
          logs={mergedLogs}
          stepResults={build.step_results ?? []}
          isStreaming={isStreaming}
          streamError={isTerminal ? undefined : (streamError ?? undefined)}
          logsUnavailable={fullLogsQuery.isError}
          isTerminal={isTerminal}
        />
      </section>

      <div className="grid items-start gap-6 xl:grid-cols-2">
        <ArtifactsPanel
          artifacts={artifactsQuery.data?.artifacts ?? []}
          isLoading={artifactsQuery.isLoading}
          buildStatus={build.status}
        />
        <EventTimeline events={events} />
      </div>
    </PageLayout>
  )
}

function isArtifactExpired(artifact: Artifact): boolean {
  if (artifact.expires_at == null) return false
  return artifact.expires_at <= Math.floor(Date.now() / 1000)
}

function artifactExpiryLabel(artifact: Artifact): string | null {
  if (artifact.expires_at == null) return null
  const now = Math.floor(Date.now() / 1000)
  if (artifact.expires_at <= now) return 'Expired'
  return `Expires ${relativeTime(artifact.expires_at)}`
}

function artifactEmptyMessage(buildStatus: string): string {
  switch (buildStatus) {
    case 'succeeded':
      return 'Build succeeded, but no files matched the pipeline artifact patterns.'
    case 'failed':
    case 'timed_out':
      return 'This build ended before it could publish artifacts.'
    case 'canceled':
      return 'This build was canceled before artifacts were published.'
    case 'expired':
      return 'Artifacts are no longer available for this expired build.'
    default:
      return 'Artifacts will appear here once the build produces them.'
  }
}

const TTL_OPTIONS = [
  { value: '3600', label: '1 hour' },
  { value: '21600', label: '6 hours' },
  { value: '86400', label: '24 hours' },
  { value: '604800', label: '7 days' },
] as const

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
  const createTokenMutation = useCreateScopedDownloadToken()

  const [shareArtifact, setShareArtifact] = useState<Artifact | null>(null)
  const [ttlSecs, setTtlSecs] = useState('86400')
  const [singleUse, setSingleUse] = useState(false)
  const [createdToken, setCreatedToken] =
    useState<CreateScopedDownloadTokenResponse | null>(null)

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

  function handleShareLink(artifact: Artifact) {
    setShareArtifact(artifact)
    setCreatedToken(null)
    setTtlSecs('86400')
    setSingleUse(false)
  }

  function handleCreateToken() {
    if (!shareArtifact) return
    createTokenMutation.mutate(
      {
        artifactId: shareArtifact.id,
        data: {
          ttl_secs: Number(ttlSecs),
          single_use: singleUse,
        },
      },
      {
        onSuccess: (res) => {
          setCreatedToken(res)
        },
        onError: (err) => {
          toast.error(`Failed to create share link: ${err.message}`)
        },
      },
    )
  }

  function handleCopyShareUrl() {
    if (!createdToken) return
    void navigator.clipboard.writeText(createdToken.download_url).then(
      () => toast.success('Share link copied to clipboard'),
      () => toast.error('Failed to copy link'),
    )
  }

  return (
    <>
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
              {artifactEmptyMessage(buildStatus)}
            </p>
          ) : (
            <div className="space-y-2">
              {artifacts.map((artifact) => {
                const expired = isArtifactExpired(artifact)
                const expiryLabel = artifactExpiryLabel(artifact)

                return (
                  <div
                    key={artifact.id}
                    className={`flex items-center gap-2 border p-2 ${expired ? 'opacity-50' : ''}`}
                  >
                    <div className="min-w-0 flex-1">
                      <p className="truncate text-xs font-medium">
                        {artifact.name}
                      </p>
                      <div className="mt-0.5 flex items-center gap-1.5">
                        <Badge
                          variant={artifactTypeBadgeVariant(
                            artifact.artifact_type,
                          )}
                          className="text-[10px]"
                        >
                          {artifact.artifact_type}
                        </Badge>
                        <span className="text-[10px] text-muted-foreground">
                          {artifact.file_size != null
                            ? formatFileSize(artifact.file_size)
                            : '—'}
                        </span>
                        {expiryLabel ? (
                          <span
                            className={`text-[10px] ${expired ? 'text-destructive' : 'text-muted-foreground'}`}
                          >
                            {expiryLabel}
                          </span>
                        ) : null}
                      </div>
                    </div>
                    <div className="flex items-center gap-0.5">
                      <Button
                        variant="ghost"
                        size="icon"
                        className="size-7 shrink-0"
                        title="Share link"
                        aria-label={`Share link for ${artifact.name}`}
                        onClick={() => handleShareLink(artifact)}
                        disabled={expired}
                      >
                        <HugeiconsIcon icon={Share08Icon} size={14} />
                      </Button>
                      <Button
                        variant="ghost"
                        size="icon"
                        className="size-7 shrink-0"
                        title="Copy download link"
                        aria-label={`Copy link for ${artifact.name}`}
                        onClick={() =>
                          handleCopyLink(artifact.id, artifact.name)
                        }
                        disabled={downloadMutation.isPending || expired}
                      >
                        <HugeiconsIcon icon={Copy01Icon} size={14} />
                      </Button>
                      <Button
                        variant="ghost"
                        size="icon"
                        className="size-7 shrink-0"
                        title="Download"
                        aria-label={`Download ${artifact.name}`}
                        onClick={() =>
                          handleDownload(artifact.id, artifact.name)
                        }
                        disabled={downloadMutation.isPending || expired}
                      >
                        <HugeiconsIcon icon={Download04Icon} size={14} />
                      </Button>
                    </div>
                  </div>
                )
              })}
            </div>
          )}
        </CardContent>
      </Card>

      {/* Share Link Dialog */}
      <Dialog
        open={shareArtifact !== null}
        onOpenChange={(open) => {
          if (!open) setShareArtifact(null)
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>
              {createdToken ? 'Share Link Created' : 'Create Share Link'}
            </DialogTitle>
            <DialogDescription>
              {createdToken
                ? 'Copy this link to share. It will not be shown again.'
                : `Generate a scoped download link for "${shareArtifact?.name}".`}
            </DialogDescription>
          </DialogHeader>

          {createdToken ? (
            <div className="space-y-3">
              <Alert>
                <AlertDescription className="break-all text-xs font-mono">
                  {createdToken.download_url}
                </AlertDescription>
              </Alert>
              <div className="flex items-center gap-2 text-xs text-muted-foreground">
                <span>Expires {relativeTime(createdToken.expires_at)}</span>
                {createdToken.single_use ? (
                  <Badge variant="secondary" className="text-[10px]">
                    Single use
                  </Badge>
                ) : null}
              </div>
              <DialogFooter>
                <Button
                  variant="secondary"
                  onClick={() => setShareArtifact(null)}
                >
                  Close
                </Button>
                <Button onClick={handleCopyShareUrl}>
                  <HugeiconsIcon
                    icon={Copy01Icon}
                    size={14}
                    className="mr-1.5"
                  />
                  Copy Link
                </Button>
              </DialogFooter>
            </div>
          ) : (
            <div className="space-y-4">
              <div className="space-y-2">
                <Label htmlFor="ttl-select">Expires after</Label>
                <Select
                  value={ttlSecs}
                  onValueChange={(v) => {
                    if (v != null) setTtlSecs(v)
                  }}
                >
                  <SelectTrigger id="ttl-select">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {TTL_OPTIONS.map((opt) => (
                      <SelectItem key={opt.value} value={opt.value}>
                        {opt.label}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
              <div className="flex items-center gap-2">
                <Checkbox
                  id="single-use"
                  checked={singleUse}
                  onCheckedChange={(checked) => setSingleUse(checked === true)}
                />
                <Label htmlFor="single-use" className="text-sm font-normal">
                  Single use (consumed after first download)
                </Label>
              </div>
              <DialogFooter>
                <Button
                  variant="secondary"
                  onClick={() => setShareArtifact(null)}
                >
                  Cancel
                </Button>
                <Button
                  onClick={handleCreateToken}
                  disabled={createTokenMutation.isPending}
                >
                  {createTokenMutation.isPending ? (
                    <>
                      <Spinner className="mr-1.5" />
                      Creating...
                    </>
                  ) : (
                    'Create Link'
                  )}
                </Button>
              </DialogFooter>
            </div>
          )}
        </DialogContent>
      </Dialog>
    </>
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
