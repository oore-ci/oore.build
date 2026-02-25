import { For, Match, Show, Switch } from 'solid-js'
import { createFileRoute } from '@tanstack/solid-router'
import {
  Download04Icon,
  File01Icon,
  GitBranchIcon,
  GitCommitIcon,
  TimeQuarterPassIcon,
} from '@hugeicons/core-free-icons'

import type { Artifact } from '@/lib/types'
import {
  getActiveInstanceOrRedirect,
  requireAuthOrRedirect,
} from '@/lib/instance-context'
import {
  isTerminalStatus,
  useArtifactDownloadLink,
  useArtifacts,
  useBuild,
  useBuildLogs,
  useCancelBuild,
} from '@/hooks/use-builds'
import { PageMeta } from '@/lib/seo'
import { getStatusVariant } from '@/lib/status-variants'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { HugeIcon } from '@/components/huge-icon'
import { PageHeader } from '@/components/page-header'
import { PageLayout } from '@/components/page-layout'
import { Skeleton } from '@/components/ui/skeleton'
import TerminalLogViewer from '@/components/terminal-log-viewer'
import { toast } from '@/components/ui/sonner'

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
  if (bytes < 1024 * 1024 * 1024)
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
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
  const params = Route.useParams()
  const buildId = () => params().buildId
  const buildQuery = useBuild(buildId())
  const artifactsQuery = useArtifacts(buildId())
  const logsQuery = useBuildLogs(buildId())
  const cancelMutation = useCancelBuild()

  const build = () => buildQuery.data?.build
  const events = () => buildQuery.data?.events ?? []
  const isTerminal = () => !!build()?.status && isTerminalStatus(build()!.status)
  const label = () =>
    build()?.build_number ? `Build #${build()!.build_number}` : 'Build Details'

  const handleCancel = () => {
    cancelMutation.mutate(buildId(), {
      onSuccess: () => {
        toast.success('Build canceled')
      },
      onError: (error) => {
        toast.error(error instanceof Error ? error.message : 'Failed to cancel build')
      },
    })
  }

  const duration = () => {
    if (!build()?.started_at) return null
    return (build()?.finished_at ?? Math.floor(Date.now() / 1000)) - build()!.started_at!
  }

  return (
    <PageLayout width="full">
      <PageMeta title={label()} noindex />

      <Switch>
        <Match when={buildQuery.isLoading}>
          <Skeleton class="h-8 w-56" />
          <Skeleton class="h-24 w-full" />
          <Skeleton class="h-64 w-full" />
        </Match>

        <Match when={!!buildQuery.error}>
          <Alert variant="destructive">
            <AlertDescription>
              Failed to load build: {buildQuery.error?.message}
            </AlertDescription>
          </Alert>
        </Match>

        <Match when={!!build()}>
          <PageHeader
            title={`Build #${build()!.build_number}`}
            back={{ to: '/builds', label: 'Builds' }}
            meta={
              <>
                <Badge variant={getStatusVariant(build()!.status)}>{build()!.status}</Badge>
                <Badge variant="outline">{build()!.trigger_type}</Badge>
                {build()!.branch ? (
                  <span class="inline-flex items-center gap-1 font-mono text-[11px]">
                    <HugeIcon icon={GitBranchIcon} size={12} />
                    {build()!.branch}
                  </span>
                ) : null}
                {build()!.commit_sha ? (
                  <span class="inline-flex items-center gap-1 font-mono text-[11px]">
                    <HugeIcon icon={GitCommitIcon} size={12} />
                    {build()!.commit_sha?.slice(0, 7)}
                  </span>
                ) : null}
                {duration() != null ? (
                  <span class="inline-flex items-center gap-1">
                    <HugeIcon icon={TimeQuarterPassIcon} size={12} />
                    {formatDuration(duration() as number)}
                  </span>
                ) : null}
                <span class="text-[oklch(0.5_0_0)]">|</span>
                <span>
                  Queued {relativeTime(build()!.queued_at)}
                  {build()!.started_at
                    ? ` → Started ${relativeTime(build()!.started_at as number)}`
                    : ''}
                  {build()!.finished_at
                    ? ` → Finished ${relativeTime(build()!.finished_at as number)}`
                    : ''}
                </span>
              </>
            }
            actions={
              !isTerminal() ? (
                <Button
                  variant="destructive"
                  size="sm"
                  onClick={handleCancel}
                  disabled={cancelMutation.isPending}
                >
                  {cancelMutation.isPending ? 'Canceling...' : 'Cancel Build'}
                </Button>
              ) : undefined
            }
          />

          <div class="grid gap-6 xl:grid-cols-[1fr_340px]">
            <div class="min-w-0">
              <TerminalLogViewer lines={logsQuery.data?.logs ?? []} />
            </div>

            <aside class="space-y-4 xl:sticky xl:top-16 xl:max-h-[calc(100vh-5rem)] xl:overflow-y-auto">
              <ArtifactsPanel
                artifacts={artifactsQuery.data?.artifacts ?? []}
                isLoading={artifactsQuery.isLoading}
              />
              <EventTimeline events={events()} />
            </aside>
          </div>
        </Match>
      </Switch>
    </PageLayout>
  )
}

function ArtifactsPanel(props: {
  artifacts: Array<Artifact>
  isLoading: boolean
}) {
  const downloadMutation = useArtifactDownloadLink()

  const handleDownload = (artifactId: string, name: string) => {
    downloadMutation.mutate(artifactId, {
      onSuccess: (response) => {
        window.open(response.download_url, '_blank', 'noopener,noreferrer')
      },
      onError: (error) => {
        toast.error(
          `Failed to get download link for ${name}: ${
            error instanceof Error ? error.message : 'unknown error'
          }`,
        )
      },
    })
  }

  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle class="flex items-center gap-2 text-sm font-medium uppercase tracking-wider text-muted-foreground">
          <HugeIcon icon={File01Icon} size={14} />
          Artifacts
          {props.artifacts.length > 0 ? (
            <Badge variant="secondary" class="text-[10px]">
              {props.artifacts.length}
            </Badge>
          ) : null}
        </CardTitle>
      </CardHeader>
      <CardContent>
        <Show
          when={!props.isLoading}
          fallback={
            <div class="space-y-2">
              <Skeleton class="h-8 w-full" />
              <Skeleton class="h-8 w-full" />
            </div>
          }
        >
          <Show
            when={props.artifacts.length > 0}
            fallback={<p class="text-xs text-muted-foreground">No artifacts yet.</p>}
          >
            <div class="space-y-2">
              <For each={props.artifacts}>
                {(artifact) => (
                  <div class="flex items-center gap-2 rounded-md border p-2">
                    <div class="min-w-0 flex-1">
                      <p class="truncate text-xs font-medium">{artifact.name}</p>
                      <div class="mt-0.5 flex items-center gap-1.5">
                        <Badge
                          variant={artifactTypeBadgeVariant(artifact.artifact_type)}
                          class="text-[10px]"
                        >
                          {artifact.artifact_type}
                        </Badge>
                        <span class="text-[10px] text-muted-foreground">
                          {artifact.file_size != null
                            ? formatFileSize(artifact.file_size)
                            : '—'}
                        </span>
                      </div>
                    </div>

                    <Button
                      variant="ghost"
                      size="icon"
                      class="size-7 shrink-0"
                      onClick={() => handleDownload(artifact.id, artifact.name)}
                      disabled={downloadMutation.isPending}
                    >
                      <HugeIcon icon={Download04Icon} size={14} />
                    </Button>
                  </div>
                )}
              </For>
            </div>
          </Show>
        </Show>
      </CardContent>
    </Card>
  )
}

function EventTimeline(props: {
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
        <CardTitle class="text-sm font-medium uppercase tracking-wider text-muted-foreground">
          Event timeline
        </CardTitle>
      </CardHeader>
      <CardContent>
        <Show
          when={props.events.length > 0}
          fallback={<p class="text-xs text-muted-foreground">No events yet.</p>}
        >
          <div class="relative space-y-0">
            <For each={props.events}>
              {(event, index) => (
                <div class="relative flex gap-3 pb-4 last:pb-0">
                  <div class="mt-1.5 flex flex-col items-center">
                    <span class="size-2 rounded-full bg-primary" />
                    {index() < props.events.length - 1 ? (
                      <span class="mt-1 block h-full w-px bg-border" />
                    ) : null}
                  </div>

                  <div class="min-w-0 flex-1">
                    <p class="text-xs font-medium">
                      {event.from_status ? `${event.from_status} → ` : ''}
                      {event.to_status}
                    </p>
                    {event.reason ? (
                      <p class="mt-0.5 text-xs text-muted-foreground">{event.reason}</p>
                    ) : null}
                    <p class="mt-0.5 text-[11px] text-muted-foreground">
                      {relativeTime(event.created_at)}
                    </p>
                  </div>
                </div>
              )}
            </For>
          </div>
        </Show>
      </CardContent>
    </Card>
  )
}
