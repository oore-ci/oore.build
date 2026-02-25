import { For, Match, Show, Switch } from 'solid-js'
import { Link, createFileRoute } from '@tanstack/solid-router'
import {
  Delete02Icon,
  Edit02Icon,
  PlayIcon,
} from '@hugeicons/core-free-icons'

import { Alert, AlertDescription } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { HugeIcon } from '@/components/huge-icon'
import { PageHeader } from '@/components/page-header'
import { PageLayout } from '@/components/page-layout'
import { Skeleton } from '@/components/ui/skeleton'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import { useBuilds } from '@/hooks/use-builds'
import { useHasPermission } from '@/hooks/use-permissions'
import { usePipeline, useUpdatePipeline } from '@/hooks/use-pipelines'
import {
  getActiveInstanceOrRedirect,
  requireAuthOrRedirect,
} from '@/lib/instance-context'
import { PageMeta } from '@/lib/seo'
import { getPipelineStatusVariant, getStatusVariant } from '@/lib/status-variants'
import { toast } from '@/components/ui/sonner'

export const Route = createFileRoute('/projects/$projectId/pipelines/$pipelineId')({
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireAuthOrRedirect(instance.id)
  },
  staticData: { breadcrumbLabel: 'Pipeline' },
  component: PipelineDetailPage,
})

function PipelineDetailPage() {
  const params = Route.useParams()
  const pipelineQuery = usePipeline(params().pipelineId)
  const buildsQuery = useBuilds({
    project_id: params().projectId,
    pipeline_id: params().pipelineId,
    limit: 20,
  })

  const updatePipeline = useUpdatePipeline()
  const canWrite = useHasPermission('pipelines', 'write')
  const canTriggerBuild = useHasPermission('builds', 'write')

  const pipeline = () => pipelineQuery.data?.pipeline
  const builds = () => buildsQuery.data?.builds ?? []

  const toggleEnabled = () => {
    if (!pipeline()) return
    updatePipeline.mutate(
      {
        pipelineId: pipeline()!.id,
        data: {
          enabled: !pipeline()!.enabled,
          name: pipeline()!.name,
          config_path: pipeline()!.config_path,
          config_path_explicit: pipeline()!.config_path_explicit,
          execution_config: pipeline()!.execution_config,
          trigger_config: pipeline()!.trigger_config,
          concurrency: pipeline()!.concurrency,
        },
      },
      {
        onSuccess: () => {
          toast.success(pipeline()!.enabled ? 'Pipeline disabled' : 'Pipeline enabled')
        },
        onError: (error) => {
          toast.error(error instanceof Error ? error.message : 'Failed to update pipeline')
        },
      },
    )
  }

  return (
    <PageLayout width="wide">
      <PageMeta title={pipeline()?.name ?? 'Pipeline'} noindex />

      <Switch>
        <Match when={pipelineQuery.isLoading}>
          <Skeleton class="h-8 w-56" />
          <Skeleton class="h-24 w-full" />
          <Skeleton class="h-64 w-full" />
        </Match>

        <Match when={!!pipelineQuery.error || !pipeline()}>
          <Alert variant="destructive">
            <AlertDescription>
              Failed to load pipeline: {pipelineQuery.error?.message ?? 'Not found'}
            </AlertDescription>
          </Alert>
        </Match>

        <Match when>
          <PageHeader
            title={pipeline()!.name}
            back={{ to: `/projects/${params().projectId}`, label: 'Project' }}
            description="Pipeline overview and configuration."
            meta={
              <>
                <Badge variant={getPipelineStatusVariant(pipeline()!.enabled)}>
                  {pipeline()!.enabled ? 'enabled' : 'disabled'}
                </Badge>
                <For each={pipeline()!.execution_config.platforms}>
                  {(platform) => (
                    <Badge variant="outline" class="font-mono text-[11px]">
                      {platform}
                    </Badge>
                  )}
                </For>
                <span>Updated {new Date(pipeline()!.updated_at * 1000).toLocaleDateString()}</span>
              </>
            }
            actions={
              <>
                <Show when={canTriggerBuild}>
                  <Link to="/builds">
                    <Button>
                      <HugeIcon icon={PlayIcon} size={16} />
                      Run Build
                    </Button>
                  </Link>
                </Show>

                <Show when={canWrite}>
                  <Button variant="outline" onClick={toggleEnabled}>
                    {pipeline()!.enabled ? 'Disable' : 'Enable'}
                  </Button>
                  <Link
                    to="/projects/$projectId/pipelines/$pipelineId/edit"
                    params={{
                      projectId: params().projectId,
                      pipelineId: params().pipelineId,
                    }}
                  >
                    <Button variant="outline">
                      <HugeIcon icon={Edit02Icon} size={16} />
                      Edit
                    </Button>
                  </Link>
                  <Button variant="destructive" disabled>
                    <HugeIcon icon={Delete02Icon} size={16} />
                    Delete
                  </Button>
                </Show>
              </>
            }
          />

          <Card>
            <CardContent class="space-y-0">
              <SectionRow
                title="Configuration"
                rows={[
                  ['Config path', pipeline()!.config_path],
                  ['Resolution', pipeline()!.config_path_explicit ? 'Explicit path only' : 'Auto detect'],
                  ['Flutter version', pipeline()!.execution_config.flutter_version ?? 'auto'],
                  ['Created', new Date(pipeline()!.created_at * 1000).toLocaleString()],
                  ['Updated', new Date(pipeline()!.updated_at * 1000).toLocaleString()],
                ]}
              />
              <SectionRow
                title="Triggers"
                rows={[
                  ['Events', pipeline()!.trigger_config.events.join(', ') || 'all'],
                  ['Branches', pipeline()!.trigger_config.branches.join(', ') || 'all'],
                  ['Cancel previous', pipeline()!.concurrency.cancel_previous ? 'yes' : 'no'],
                  ['Max concurrent', String(pipeline()!.concurrency.max_concurrent ?? 'unlimited')],
                ]}
              />
              <SectionRow
                title="Execution config"
                rows={[
                  ['Pre-build', pipeline()!.execution_config.commands.pre_build.join(' && ') || 'none'],
                  ['Build', pipeline()!.execution_config.commands.build.join(' && ') || 'none'],
                  ['Post-build', pipeline()!.execution_config.commands.post_build.join(' && ') || 'none'],
                  ['Artifacts', pipeline()!.execution_config.artifact_patterns.join(', ') || 'none'],
                ]}
              />
              <SectionRow
                title="Android signing"
                rows={[[
                  'Status',
                  pipeline()!.execution_config.android_signing_release_enabled
                    ? 'configured'
                    : 'not configured',
                ]]}
                last
              />
            </CardContent>
          </Card>

          <Card>
            <CardHeader>
              <CardTitle class="text-sm font-medium uppercase tracking-wider text-muted-foreground">
                Recent builds
              </CardTitle>
            </CardHeader>
            <CardContent>
              <Show
                when={builds().length > 0}
                fallback={
                  <p class="py-4 text-sm text-muted-foreground">No builds yet for this pipeline.</p>
                }
              >
                <Table>
                  <TableHeader>
                    <TableRow>
                      <TableHead>Build</TableHead>
                      <TableHead>Status</TableHead>
                      <TableHead>Branch</TableHead>
                      <TableHead>Created</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    <For each={builds()}>
                      {(build) => (
                        <TableRow>
                          <TableCell>
                            <Link
                              to="/builds/$buildId"
                              params={{ buildId: build.id }}
                              class="font-mono text-sm hover:underline"
                            >
                              #{build.build_number}
                            </Link>
                          </TableCell>
                          <TableCell>
                            <Badge variant={getStatusVariant(build.status)}>
                              {build.status}
                            </Badge>
                          </TableCell>
                          <TableCell class="font-mono text-xs text-muted-foreground">
                            {build.branch ?? 'n/a'}
                          </TableCell>
                          <TableCell class="text-sm text-muted-foreground">
                            {new Date(build.created_at * 1000).toLocaleString()}
                          </TableCell>
                        </TableRow>
                      )}
                    </For>
                  </TableBody>
                </Table>
              </Show>
            </CardContent>
          </Card>
        </Match>
      </Switch>
    </PageLayout>
  )
}

function SectionRow(props: {
  title: string
  rows: Array<[string, string]>
  last?: boolean
}) {
  return (
    <div class={`border-b ${props.last ? 'border-b-0' : ''}`}>
      <div class="flex items-center justify-between px-4 py-3">
        <p class="text-sm font-medium">{props.title}</p>
      </div>
      <div class="grid grid-cols-[120px_1fr] gap-x-4 gap-y-2 px-4 pb-4 text-sm">
        <For each={props.rows}>
          {(row) => (
            <>
              <p class="text-muted-foreground">{row[0]}</p>
              <p class="font-mono text-xs leading-5 text-foreground">{row[1]}</p>
            </>
          )}
        </For>
      </div>
    </div>
  )
}
