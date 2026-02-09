import { createFileRoute } from '@tanstack/react-router'
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  AlertCircleIcon,
  ArrowDown01Icon,
  Clock01Icon,
  Download04Icon,
  File01Icon,
  GitBranchIcon,
  GitCommitIcon,
  InformationCircleIcon,
  Loading03Icon,
  PlayIcon,
  StopIcon,
  TimeQuarterPassIcon,
} from '@hugeicons/core-free-icons'
import { toast } from 'sonner'

import type { Artifact, BuildLogChunk, BuildStatus, StepResult } from '@/lib/types'
import { getActiveInstanceOrRedirect, requireAuthOrRedirect } from '@/lib/instance-context'
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
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/components/ui/card'
import { Separator } from '@/components/ui/separator'
import { Skeleton } from '@/components/ui/skeleton'
import { Alert, AlertDescription } from '@/components/ui/alert'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import PageLayout from '@/components/page-layout'
import PageHeader from '@/components/page-header'
import { webPageTitle } from '@/lib/seo'

export const Route = createFileRoute('/builds/$buildId')({
  staticData: { breadcrumbLabel: 'Details' },
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireAuthOrRedirect(instance.id)
  },
  component: BuildDetailPage,
})

function formatDuration(seconds: number): string {
  if (seconds < 60) return `${Math.round(seconds)}s`
  const mins = Math.floor(seconds / 60)
  const secs = Math.round(seconds % 60)
  if (mins < 60) return `${mins}m ${secs}s`
  const hrs = Math.floor(mins / 60)
  const remainMins = mins % 60
  return `${hrs}h ${remainMins}m`
}

function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`
}

function relativeTime(epochSecs: number): string {
  const diffSecs = Math.floor(Date.now() / 1000) - epochSecs
  if (diffSecs < 5) return 'just now'
  if (diffSecs < 60) return `${diffSecs}s ago`
  const mins = Math.floor(diffSecs / 60)
  if (mins < 60) return `${mins}m ago`
  const hrs = Math.floor(mins / 60)
  if (hrs < 24) return `${hrs}h ago`
  const days = Math.floor(hrs / 24)
  return `${days}d ago`
}

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
  const [knownTerminal, setKnownTerminal] = useState(false)
  const buildQuery = useBuild(buildId, {
    refetchInterval: knownTerminal ? false : 3000,
  })
  const { data, isLoading, error, refetch: refetchBuild } = buildQuery
  const artifactsQuery = useArtifacts(buildId, {
    refetchInterval: knownTerminal ? false : 3000,
  })
  const { refetch: refetchArtifacts } = artifactsQuery
  const cancelMutation = useCancelBuild()

  const buildStatus = data?.build.status
  const isTerminal = buildStatus ? isTerminalStatus(buildStatus) : false

  useEffect(() => {
    if (isTerminal) setKnownTerminal(true)
  }, [isTerminal])

  useEffect(() => {
    const label = data?.build.build_number
      ? `Build #${data.build.build_number}`
      : 'Build Details'
    document.title = webPageTitle(label)
  }, [data?.build.build_number])

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

  const handleStreamDone = useCallback(() => {
    void refetchBuild()
    void refetchArtifacts()
  }, [refetchBuild, refetchArtifacts])

  if (isLoading) {
    return (
      <PageLayout width="wide">
        <Skeleton className="h-8 w-56" />
        <Skeleton className="h-24 w-full" />
        <Skeleton className="h-64 w-full" />
      </PageLayout>
    )
  }

  if (error) {
    return (
      <PageLayout width="wide">
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
  const duration =
    build.started_at
      ? ((build.finished_at ?? Math.floor(Date.now() / 1000)) - build.started_at)
      : null
  const snapshot = build.config_snapshot as Record<string, unknown>
  const configPath =
    typeof snapshot.config_path === 'string' ? snapshot.config_path : null
  const resolutionPolicy =
    typeof snapshot.config_resolution_policy === 'string'
      ? snapshot.config_resolution_policy
      : null

  return (
    <PageLayout width="wide">
      <PageHeader
        title={`Build #${build.build_number}`}
        back={{ to: '/builds', label: 'Builds' }}
        description="Execution status, logs, artifacts, and event timeline."
        meta={
          <>
            <Badge variant={getStatusVariant(build.status)}>{build.status}</Badge>
            <Badge variant="outline">{build.trigger_type}</Badge>
            {build.branch ? (
              <Badge variant="outline" className="font-mono text-[11px]">
                {build.branch}
              </Badge>
            ) : null}
            {duration != null ? (
              <span className="inline-flex items-center gap-1">
                <HugeiconsIcon icon={TimeQuarterPassIcon} size={12} />
                {formatDuration(duration)}
              </span>
            ) : null}
          </>
        }
        actions={
          canCancel ? (
            <Button
              variant="destructive"
              onClick={handleCancel}
              disabled={cancelMutation.isPending}
            >
              {cancelMutation.isPending ? 'Canceling...' : 'Cancel Build'}
            </Button>
          ) : undefined
        }
      />

      <section className="grid gap-4 md:grid-cols-2">
        <Card>
          <CardHeader>
            <CardTitle className="text-base">Build Details</CardTitle>
            <CardDescription>Build identity and trigger context</CardDescription>
          </CardHeader>
          <CardContent>
            <div className="space-y-3 text-sm">
              <div className="flex items-center justify-between gap-4">
                <span className="inline-flex items-center gap-2 text-muted-foreground">
                  <HugeiconsIcon icon={GitBranchIcon} size={14} />
                  Branch
                </span>
                <code className="font-mono text-xs">{build.branch ?? 'n/a'}</code>
              </div>
              <Separator />
              <div className="flex items-center justify-between gap-4">
                <span className="inline-flex items-center gap-2 text-muted-foreground">
                  <HugeiconsIcon icon={GitCommitIcon} size={14} />
                  Commit
                </span>
                <code className="font-mono text-xs">{build.commit_sha ?? 'n/a'}</code>
              </div>
              <Separator />
              <div className="flex items-center justify-between gap-4">
                <span className="text-muted-foreground">Runner</span>
                <span className="font-mono text-xs">{build.runner_id ?? 'unassigned'}</span>
              </div>
              <Separator />
              <div className="flex items-center justify-between gap-4">
                <span className="text-muted-foreground">Trigger</span>
                <span className="capitalize">{build.trigger_type.replaceAll('_', ' ')}</span>
              </div>
              <Separator />
              <div className="flex items-center justify-between gap-4">
                <span className="text-muted-foreground">Build ID</span>
                <span className="font-mono text-xs">{build.id}</span>
              </div>
              {configPath ? (
                <>
                  <Separator />
                  <div className="flex items-center justify-between gap-4">
                    <span className="text-muted-foreground">Config path</span>
                    <span className="font-mono text-xs">{configPath}</span>
                  </div>
                </>
              ) : null}
              {resolutionPolicy ? (
                <>
                  <Separator />
                  <div className="flex items-center justify-between gap-4">
                    <span className="text-muted-foreground">Policy</span>
                    <Badge variant="outline" className="font-mono text-[11px]">
                      {resolutionPolicy}
                    </Badge>
                  </div>
                </>
              ) : null}
              {build.exit_code != null && build.exit_code !== 0 ? (
                <>
                  <Separator />
                  <div className="flex items-start justify-between gap-4">
                    <span className="inline-flex items-center gap-2 text-destructive">
                      <HugeiconsIcon icon={AlertCircleIcon} size={14} />
                      Exit code
                    </span>
                    <span className="font-mono text-xs text-destructive">{build.exit_code}</span>
                  </div>
                </>
              ) : null}
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle className="text-base">Timing</CardTitle>
            <CardDescription>Build lifecycle timestamps</CardDescription>
          </CardHeader>
          <CardContent>
            <div className="space-y-3 text-sm">
              <div className="flex items-center justify-between gap-4">
                <span className="inline-flex items-center gap-2 text-muted-foreground">
                  <HugeiconsIcon icon={Clock01Icon} size={14} />
                  Queued
                </span>
                <div className="text-right">
                  <div>{new Date(build.queued_at * 1000).toLocaleString()}</div>
                  <div className="text-xs text-muted-foreground">{relativeTime(build.queued_at)}</div>
                </div>
              </div>
              <Separator />
              <div className="flex items-center justify-between gap-4">
                <span className="inline-flex items-center gap-2 text-muted-foreground">
                  <HugeiconsIcon icon={PlayIcon} size={14} />
                  Started
                </span>
                {build.started_at ? (
                  <div className="text-right">
                    <div>{new Date(build.started_at * 1000).toLocaleString()}</div>
                    <div className="text-xs text-muted-foreground">{relativeTime(build.started_at)}</div>
                  </div>
                ) : (
                  <span className="text-muted-foreground">Not started</span>
                )}
              </div>
              <Separator />
              <div className="flex items-center justify-between gap-4">
                <span className="inline-flex items-center gap-2 text-muted-foreground">
                  <HugeiconsIcon icon={StopIcon} size={14} />
                  Finished
                </span>
                {build.finished_at ? (
                  <div className="text-right">
                    <div>{new Date(build.finished_at * 1000).toLocaleString()}</div>
                    <div className="text-xs text-muted-foreground">{relativeTime(build.finished_at)}</div>
                  </div>
                ) : (
                  <span className="text-muted-foreground">
                    {isTerminal ? 'Not finished' : 'In progress...'}
                  </span>
                )}
              </div>
              {duration != null ? (
                <>
                  <Separator />
                  <div className="flex items-center justify-between gap-4">
                    <span className="text-muted-foreground">Duration</span>
                    <span className="font-medium">{formatDuration(duration)}</span>
                  </div>
                </>
              ) : null}
            </div>
          </CardContent>
        </Card>
      </section>

      <BuildLogsCard
        buildId={buildId}
        buildStatus={build.status}
        stepResults={build.step_results ?? []}
        onStreamDone={handleStreamDone}
      />

      <ArtifactsCard
        artifacts={artifactsQuery.data?.artifacts ?? []}
        isLoading={artifactsQuery.isLoading}
      />

      <Card>
        <CardHeader>
          <CardTitle className="text-base">Event timeline</CardTitle>
        </CardHeader>
        <CardContent>
          {events.length === 0 ? (
            <p className="text-sm text-muted-foreground">No events yet.</p>
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Time</TableHead>
                  <TableHead>Transition</TableHead>
                  <TableHead>Reason</TableHead>
                  <TableHead>Actor</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {events.map((event) => (
                  <TableRow key={event.id}>
                    <TableCell
                      className="text-xs text-muted-foreground"
                      title={new Date(event.created_at * 1000).toLocaleString()}
                    >
                      {relativeTime(event.created_at)}
                    </TableCell>
                    <TableCell>
                      <span className="text-sm">
                        {event.from_status ? (
                          <span className="text-muted-foreground">{event.from_status} → </span>
                        ) : null}
                        <span className="font-medium">{event.to_status}</span>
                      </span>
                    </TableCell>
                    <TableCell className="text-sm text-muted-foreground">{event.reason ?? '—'}</TableCell>
                    <TableCell className="text-sm text-muted-foreground">{event.actor ?? '—'}</TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )}
        </CardContent>
      </Card>
    </PageLayout>
  )
}

function BuildLogsCard({
  buildId,
  buildStatus,
  stepResults,
  onStreamDone,
}: {
  buildId: string
  buildStatus: BuildStatus
  stepResults: Array<StepResult>
  onStreamDone: () => void
}) {
  type StepGroup = {
    name: string
    status: string
    command?: string
    durationMs?: number
    logs: Array<BuildLogChunk>
  }

  const isTerminal = isTerminalStatus(buildStatus)
  const streamEnabled = !isTerminal
  const {
    logs: streamLogs,
    isStreaming,
    error: streamError,
  } = useLogStream(buildId, streamEnabled, { onDone: onStreamDone })
  const { data: fullLogsData, isLoading: logsLoading } = useBuildLogs(buildId)

  const [selectedStep, setSelectedStep] = useState<string>('')
  const [autoScroll, setAutoScroll] = useState(true)
  const scrollContainerRef = useRef<HTMLDivElement>(null)

  const logs: Array<BuildLogChunk> = useMemo(() => {
    if (streamEnabled && streamLogs.length > 0) return streamLogs
    if (isTerminal && fullLogsData?.logs) return fullLogsData.logs
    if (streamLogs.length > 0) return streamLogs
    return fullLogsData?.logs ?? []
  }, [streamEnabled, streamLogs, isTerminal, fullLogsData?.logs])

  const { stepGroups, allVisibleLogs } = useMemo(() => {
    const groups = new Map<string, StepGroup>()
    const order: Array<string> = []
    const visibleLogs: Array<BuildLogChunk> = []
    let activeStep: string | null = null

    const ensureGroup = (name: string): StepGroup => {
      const existing = groups.get(name)
      if (existing) return existing
      const created: StepGroup = {
        name,
        status: 'pending',
        logs: [],
      }
      groups.set(name, created)
      order.push(name)
      return created
    }

    for (const chunk of logs) {
      const marker = parseStepMarker(chunk.content)
      if (marker) {
        const group = ensureGroup(marker.name)
        if (marker.event === 'start') {
          group.status = 'running'
          if (marker.command) group.command = marker.command
          activeStep = marker.name
        } else {
          group.status = marker.status ?? 'succeeded'
          activeStep = activeStep === marker.name ? null : activeStep
        }
        continue
      }

      visibleLogs.push(chunk)
      if (activeStep) {
        ensureGroup(activeStep).logs.push(chunk)
      }
    }

    for (const result of stepResults) {
      const group = ensureGroup(result.name)
      group.status = result.status
      group.durationMs = result.duration_ms
    }

    return {
      stepGroups: order
        .map((name) => groups.get(name))
        .filter(Boolean) as Array<StepGroup>,
      allVisibleLogs: visibleLogs,
    }
  }, [logs, stepResults])

  useEffect(() => {
    const hasSelected =
      selectedStep === 'all' || stepGroups.some((group) => group.name === selectedStep)
    if (hasSelected) return
    const running = stepGroups.find((group) => group.status === 'running')
    setSelectedStep(running?.name ?? (stepGroups[0]?.name ?? 'all'))
  }, [stepGroups, selectedStep])

  const selectedLogs = useMemo(() => {
    if (selectedStep === 'all') return allVisibleLogs
    return stepGroups.find((group) => group.name === selectedStep)?.logs ?? []
  }, [selectedStep, allVisibleLogs, stepGroups])

  const selectedStepMeta = useMemo(() => {
    if (selectedStep === 'all') return null
    const group = stepGroups.find((entry) => entry.name === selectedStep)
    if (!group) return null
    const envPreview = group.logs.find((chunk) =>
      chunk.content.startsWith('# env: '),
    )?.content
    return {
      command: group.command,
      envPreview,
    }
  }, [selectedStep, stepGroups])

  const currentStep = stepGroups.find((group) => group.status === 'running')

  useEffect(() => {
    if (autoScroll && scrollContainerRef.current) {
      scrollContainerRef.current.scrollTop = scrollContainerRef.current.scrollHeight
    }
  }, [selectedLogs, autoScroll])

  const handleScroll = useCallback(() => {
    const el = scrollContainerRef.current
    if (!el) return
    const isAtBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 40
    setAutoScroll(isAtBottom)
  }, [])

  return (
    <Card>
      <CardHeader>
        <div className="flex items-center justify-between gap-2">
          <CardTitle className="flex items-center gap-2 text-base">
            Build logs
            {isStreaming ? (
              <span className="flex items-center gap-1 text-xs font-normal text-muted-foreground">
                <HugeiconsIcon icon={Loading03Icon} size={12} className="animate-spin" />
                Streaming
              </span>
            ) : null}
          </CardTitle>
          {!autoScroll && selectedLogs.length > 0 ? (
            <Button
              variant="ghost"
              size="sm"
              onClick={() => {
                setAutoScroll(true)
                if (scrollContainerRef.current) {
                  scrollContainerRef.current.scrollTop = scrollContainerRef.current.scrollHeight
                }
              }}
            >
              <HugeiconsIcon icon={ArrowDown01Icon} size={14} />
              Scroll to bottom
            </Button>
          ) : null}
        </div>
        {currentStep ? (
          <p className="text-xs text-muted-foreground">
            Current step: <span className="font-medium">{currentStep.name}</span>
          </p>
        ) : null}
      </CardHeader>
      <CardContent>
        <div
          className={
            stepGroups.length > 0
              ? 'grid gap-4 lg:grid-cols-[280px_minmax(0,1fr)]'
              : 'space-y-3'
          }
        >
          {stepGroups.length > 0 ? (
            <aside className="space-y-2 rounded-md border bg-muted/20 p-2">
              <Button
                size="sm"
                variant={selectedStep === 'all' ? 'default' : 'ghost'}
                className="w-full justify-between"
                onClick={() => setSelectedStep('all')}
              >
                <span>All logs</span>
                <span className="text-xs text-muted-foreground">{allVisibleLogs.length}</span>
              </Button>
              {stepGroups.map((group) => (
                <Button
                  key={group.name}
                  size="sm"
                  variant={selectedStep === group.name ? 'default' : 'ghost'}
                  className="w-full justify-between gap-2"
                  onClick={() => setSelectedStep(group.name)}
                  title={group.command}
                >
                  <span className="truncate">{group.name}</span>
                  <span className="inline-flex items-center gap-2">
                    <Badge variant={stepStatusVariant(group.status)}>{group.status}</Badge>
                    {group.durationMs != null ? (
                      <span className="text-[10px] text-muted-foreground">
                        {formatDuration(group.durationMs / 1000)}
                      </span>
                    ) : null}
                  </span>
                </Button>
              ))}
            </aside>
          ) : null}

          <div className="space-y-3">
            {streamError ? (
              <Alert>
                <HugeiconsIcon icon={InformationCircleIcon} size={14} />
                <AlertDescription>{streamError}</AlertDescription>
              </Alert>
            ) : null}

            {selectedStepMeta?.command ? (
              <div className="space-y-2 rounded-md border bg-muted/30 p-3">
                <div>
                  <p className="text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
                    Command
                  </p>
                  <p className="font-mono text-xs">$ {selectedStepMeta.command}</p>
                </div>
                {selectedStepMeta.envPreview ? (
                  <div>
                    <p className="text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
                      Env
                    </p>
                    <p className="font-mono text-xs">{selectedStepMeta.envPreview}</p>
                  </div>
                ) : null}
              </div>
            ) : null}

            {logsLoading && !streamEnabled ? (
              <div className="space-y-2">
                <Skeleton className="h-4 w-full" />
                <Skeleton className="h-4 w-3/4" />
                <Skeleton className="h-4 w-5/6" />
              </div>
            ) : selectedLogs.length === 0 ? (
              <p className="text-sm text-muted-foreground">No logs yet.</p>
            ) : (
              <div
                ref={scrollContainerRef}
                onScroll={handleScroll}
                className="max-h-[600px] overflow-y-auto border bg-muted/30 p-4"
              >
                <pre className="font-mono text-xs leading-relaxed">
                  {selectedLogs.map((chunk) => (
                    <LogLine key={chunk.sequence} chunk={chunk} />
                  ))}
                </pre>
              </div>
            )}
          </div>
        </div>
      </CardContent>
    </Card>
  )
}

function parseStepMarker(content: string): {
  event: 'start' | 'end'
  name: string
  status?: string
  command?: string
} | null {
  const prefix = '[oore-step] '
  if (!content.startsWith(prefix)) return null
  try {
    const raw = content.slice(prefix.length)
    const parsed = JSON.parse(raw) as {
      event?: string
      name?: string
      status?: string
      command?: string
    }
    if (
      (parsed.event === 'start' || parsed.event === 'end') &&
      parsed.name?.trim()
    ) {
      return {
        event: parsed.event,
        name: parsed.name.trim(),
        status: parsed.status?.trim(),
        command: parsed.command?.trim(),
      }
    }
    return null
  } catch {
    return null
  }
}

function stepStatusVariant(status: string) {
  const normalized = status.trim().toLowerCase()
  if (normalized === 'running') return 'info'
  if (normalized === 'succeeded') return 'success'
  if (normalized === 'failed' || normalized === 'canceled' || normalized === 'timed_out') {
    return 'destructive'
  }
  return 'outline'
}

function LogLine({ chunk }: { chunk: BuildLogChunk }) {
  const isStderr = chunk.stream === 'stderr'
  return (
    <div className={`flex gap-3 ${isStderr ? 'text-destructive' : 'text-foreground'}`}>
      <span className="w-8 shrink-0 select-none text-right text-muted-foreground">
        {chunk.sequence}
      </span>
      <span className="whitespace-pre-wrap break-all">{chunk.content}</span>
    </div>
  )
}

function ArtifactsCard({
  artifacts,
  isLoading,
}: {
  artifacts: Array<Artifact>
  isLoading: boolean
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

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2 text-base">
          <HugeiconsIcon icon={File01Icon} size={16} />
          Artifacts
        </CardTitle>
      </CardHeader>
      <CardContent>
        {isLoading ? (
          <div className="space-y-2">
            <Skeleton className="h-10 w-full" />
            <Skeleton className="h-10 w-full" />
          </div>
        ) : !artifacts.length ? (
          <p className="text-sm text-muted-foreground">No artifacts.</p>
        ) : (
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Name</TableHead>
                <TableHead>Type</TableHead>
                <TableHead>Size</TableHead>
                <TableHead>Checksum</TableHead>
                <TableHead className="text-right">Action</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {artifacts.map((artifact) => (
                <TableRow key={artifact.id}>
                  <TableCell className="font-medium">{artifact.name}</TableCell>
                  <TableCell>
                    <Badge variant={artifactTypeBadgeVariant(artifact.artifact_type)}>
                      {artifact.artifact_type}
                    </Badge>
                  </TableCell>
                  <TableCell>
                    {artifact.file_size != null ? formatFileSize(artifact.file_size) : '--'}
                  </TableCell>
                  <TableCell>
                    {artifact.checksum ? (
                      <span className="font-mono text-xs" title={artifact.checksum}>
                        {artifact.checksum.slice(0, 12)}...
                      </span>
                    ) : (
                      '--'
                    )}
                  </TableCell>
                  <TableCell className="text-right">
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() => handleDownload(artifact.id, artifact.name)}
                      disabled={downloadMutation.isPending}
                    >
                      <HugeiconsIcon icon={Download04Icon} size={14} />
                      Download
                    </Button>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
      </CardContent>
    </Card>
  )
}
