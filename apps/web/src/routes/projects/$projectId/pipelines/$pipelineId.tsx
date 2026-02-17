import { useState } from 'react'
import { Link, createFileRoute, useNavigate } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  ArrowDown01Icon,
  ArrowRight01Icon,
  Delete02Icon,
  Edit02Icon,
  InformationCircleIcon,
  PlayIcon,
} from '@hugeicons/core-free-icons'
import { toast } from 'sonner'

import {
  getActiveInstanceOrRedirect,
  requireAuthOrRedirect,
} from '@/lib/instance-context'
import { useBuilds } from '@/hooks/use-builds'
import { useRepositoryProvider } from '@/hooks/use-integrations'
import { useHasPermission } from '@/hooks/use-permissions'
import {
  useDeletePipeline,
  usePipeline,
  usePipelineAndroidSigning,
  usePipelineIosSigning,
  useUpdatePipeline,
} from '@/hooks/use-pipelines'
import { useProject } from '@/hooks/use-projects'
import {
  getPipelineStatusVariant,
  getStatusVariant,
} from '@/lib/status-variants'
import { relativeTime } from '@/lib/format-utils'
import { PageMeta } from '@/lib/seo'
import { Alert, AlertDescription } from '@/components/ui/alert'
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent } from '@/components/ui/card'
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible'
import PageHeader from '@/components/page-header'
import PageLayout from '@/components/page-layout'
import { Skeleton } from '@/components/ui/skeleton'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import TriggerBuildDialog from '@/components/trigger-build-dialog'

export const Route = createFileRoute(
  '/projects/$projectId/pipelines/$pipelineId',
)({
  staticData: { breadcrumbLabel: 'Pipeline' },
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireAuthOrRedirect(instance.id)
  },
  component: PipelineDetailPage,
})

/* ------------------------------------------------------------------ */
/*  Collapsible section helper                                         */
/* ------------------------------------------------------------------ */

function Section({
  title,
  defaultOpen = false,
  children,
}: {
  title: string
  defaultOpen?: boolean
  children: React.ReactNode
}) {
  const [open, setOpen] = useState(defaultOpen)
  return (
    <Collapsible open={open} onOpenChange={setOpen}>
      <CollapsibleTrigger className="flex w-full items-center gap-2 py-2 text-sm font-medium">
        <HugeiconsIcon
          icon={open ? ArrowDown01Icon : ArrowRight01Icon}
          size={14}
        />
        {title}
      </CollapsibleTrigger>
      <CollapsibleContent>
        <div className="pb-2">{children}</div>
      </CollapsibleContent>
    </Collapsible>
  )
}

/* ------------------------------------------------------------------ */
/*  Key-value row for read-only display                                */
/* ------------------------------------------------------------------ */

function KV({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex gap-4 py-1 text-xs">
      <span className="w-40 shrink-0 text-muted-foreground">{label}</span>
      <span className="min-w-0 break-all">{children}</span>
    </div>
  )
}

/* ------------------------------------------------------------------ */
/*  Main page                                                          */
/* ------------------------------------------------------------------ */

function PipelineDetailPage() {
  const { projectId, pipelineId } = Route.useParams()
  const navigate = useNavigate()
  const { data, isLoading, error } = usePipeline(pipelineId)
  const signingQuery = usePipelineAndroidSigning(pipelineId)
  const iosSigningQuery = usePipelineIosSigning(pipelineId)
  const { data: projectData } = useProject(projectId)
  const { data: buildsData } = useBuilds({
    pipeline_id: pipelineId,
    limit: 20,
  })
  const updateMutation = useUpdatePipeline()
  const deleteMutation = useDeletePipeline()
  const canWrite = useHasPermission('pipelines', 'write')
  const canDelete = useHasPermission('pipelines', 'delete')
  const canTriggerBuild = useHasPermission('builds', 'write')
  const repoProviderQuery = useRepositoryProvider(
    projectData?.project.repository_id,
  )

  const [deleteOpen, setDeleteOpen] = useState(false)
  const [triggerBuildOpen, setTriggerBuildOpen] = useState(false)

  const label = data?.pipeline.name ?? 'Pipeline Details'

  if (isLoading) {
    return (
      <PageLayout width="wide">
        <PageMeta title={label} noindex />
        <Skeleton className="h-8 w-56" />
        <Skeleton className="h-28 w-full" />
        <Skeleton className="h-56 w-full" />
      </PageLayout>
    )
  }

  if (error) {
    return (
      <PageLayout width="wide">
        <PageMeta title={label} noindex />
        <Alert variant="destructive">
          <HugeiconsIcon icon={InformationCircleIcon} size={16} />
          <AlertDescription>
            Failed to load pipeline: {error.message}
          </AlertDescription>
        </Alert>
      </PageLayout>
    )
  }

  if (!data) return null

  const { pipeline } = data
  const builds = buildsData?.builds ?? []
  const projectHasSource = !!projectData?.project.repository_id
  const manualOnlyTriggers = repoProviderQuery.data === 'local_git'

  function handleToggleEnabled() {
    updateMutation.mutate(
      { pipelineId: pipeline.id, data: { enabled: !pipeline.enabled } },
      {
        onSuccess: () =>
          toast.success(
            pipeline.enabled ? 'Pipeline disabled' : 'Pipeline enabled',
          ),
        onError: (err) =>
          toast.error(`Failed to update pipeline: ${err.message}`),
      },
    )
  }

  function handleDelete() {
    deleteMutation.mutate(pipelineId, {
      onSuccess: () => {
        toast.success('Pipeline deleted')
        void navigate({ to: '/projects/$projectId', params: { projectId } })
      },
      onError: (err) =>
        toast.error(`Failed to delete pipeline: ${err.message}`),
    })
  }

  return (
    <PageLayout width="wide">
      <PageMeta title={label} noindex />
      <PageHeader
        title={pipeline.name}
        back={{ to: `/projects/${projectId}`, label: 'Project' }}
        description="Pipeline overview and configuration."
        meta={
          <>
            <Badge variant={getPipelineStatusVariant(pipeline.enabled)}>
              {pipeline.enabled ? 'enabled' : 'disabled'}
            </Badge>
            {pipeline.execution_config.platforms.map((p) => (
              <Badge key={p} variant="outline" className="text-[11px]">
                {p}
              </Badge>
            ))}
            <span>Updated {relativeTime(pipeline.updated_at)}</span>
          </>
        }
        actions={
          canWrite || canDelete || canTriggerBuild ? (
            <>
              {canTriggerBuild ? (
                <Button
                  onClick={() => setTriggerBuildOpen(true)}
                  disabled={!projectHasSource}
                >
                  <HugeiconsIcon icon={PlayIcon} size={16} />
                  Run Build
                </Button>
              ) : null}
              {canWrite ? (
                <Button
                  variant="outline"
                  onClick={handleToggleEnabled}
                  disabled={updateMutation.isPending}
                >
                  {pipeline.enabled ? 'Disable' : 'Enable'}
                </Button>
              ) : null}
              {canWrite ? (
                <Button
                  variant="outline"
                  render={
                    <Link
                      to="/projects/$projectId/pipelines/$pipelineId/edit"
                      params={{ projectId, pipelineId }}
                    />
                  }
                  nativeButton={false}
                >
                  <HugeiconsIcon icon={Edit02Icon} size={16} />
                  Edit
                </Button>
              ) : null}
              {canDelete ? (
                <Button
                  variant="destructive"
                  onClick={() => setDeleteOpen(true)}
                >
                  <HugeiconsIcon icon={Delete02Icon} size={16} />
                  Delete
                </Button>
              ) : null}
            </>
          ) : undefined
        }
      />
      {!projectHasSource ? (
        <Alert variant="destructive">
          <HugeiconsIcon icon={InformationCircleIcon} size={16} />
          <AlertDescription>
            This project has no linked source repository. Link a repository
            before triggering builds.
          </AlertDescription>
        </Alert>
      ) : null}

      {/* Collapsible config sections */}
      <Card>
        <CardContent className="divide-y">
          <Section title="Configuration" defaultOpen>
            <KV label="Config path">
              <span className="font-mono">{pipeline.config_path}</span>
            </KV>
            <KV label="Resolution">
              {pipeline.config_path_explicit
                ? 'Explicit path only'
                : 'Auto-detect .oore.yaml / .oore.yml'}
            </KV>
            <KV label="Flutter version">
              <span className="font-mono">
                {pipeline.execution_config.flutter_version || 'auto'}
              </span>
            </KV>
            <KV label="Created">
              {new Date(pipeline.created_at * 1000).toLocaleString()}
            </KV>
            <KV label="Updated">
              {new Date(pipeline.updated_at * 1000).toLocaleString()}
            </KV>
          </Section>

          <Section title="Triggers">
            {manualOnlyTriggers ? (
              <KV label="Mode">manual only</KV>
            ) : (
              <>
                <KV label="Events">
                  {pipeline.trigger_config.events.length > 0 ? (
                    <div className="flex flex-wrap gap-1">
                      {pipeline.trigger_config.events.map((e) => (
                        <Badge
                          key={e}
                          variant="outline"
                          className="text-[11px]"
                        >
                          {e}
                        </Badge>
                      ))}
                    </div>
                  ) : (
                    'all events'
                  )}
                </KV>
                <KV label="Branch patterns">
                  {pipeline.trigger_config.branches.length > 0 ? (
                    <div className="flex flex-wrap gap-1">
                      {pipeline.trigger_config.branches.map((b) => (
                        <Badge
                          key={b}
                          variant="outline"
                          className="font-mono text-[11px]"
                        >
                          {b}
                        </Badge>
                      ))}
                    </div>
                  ) : (
                    'all branches'
                  )}
                </KV>
              </>
            )}
            <KV label="Cancel previous">
              {pipeline.concurrency.cancel_previous ? 'yes' : 'no'}
            </KV>
            <KV label="Max concurrent">
              {pipeline.concurrency.max_concurrent ?? 'unlimited'}
            </KV>
          </Section>

          <Section title="Execution config">
            <KV label="Pre-build">
              <span className="font-mono">
                {pipeline.execution_config.commands.pre_build.length > 0
                  ? pipeline.execution_config.commands.pre_build.join(' && ')
                  : 'none'}
              </span>
            </KV>
            <KV label="Build">
              <span className="font-mono">
                {pipeline.execution_config.commands.build.length > 0
                  ? pipeline.execution_config.commands.build.join(' && ')
                  : 'none'}
              </span>
            </KV>
            <KV label="Post-build">
              <span className="font-mono">
                {pipeline.execution_config.commands.post_build.length > 0
                  ? pipeline.execution_config.commands.post_build.join(' && ')
                  : 'none'}
              </span>
            </KV>
            {(pipeline.execution_config.platform_build_args?.android.length ??
              0) > 0 ? (
              <KV label="Android args">
                <span className="font-mono">
                  {pipeline.execution_config.platform_build_args?.android.join(
                    ' ',
                  )}
                </span>
              </KV>
            ) : null}
            {(pipeline.execution_config.platform_build_args?.ios.length ?? 0) >
            0 ? (
              <KV label="iOS args">
                <span className="font-mono">
                  {pipeline.execution_config.platform_build_args?.ios.join(' ')}
                </span>
              </KV>
            ) : null}
            {(pipeline.execution_config.platform_build_args?.macos.length ??
              0) > 0 ? (
              <KV label="macOS args">
                <span className="font-mono">
                  {pipeline.execution_config.platform_build_args?.macos.join(
                    ' ',
                  )}
                </span>
              </KV>
            ) : null}
            {pipeline.execution_config.platform_commands?.android ||
            pipeline.execution_config.platform_commands?.ios ||
            pipeline.execution_config.platform_commands?.macos ? (
              <KV label="Command overrides">
                <span className="font-mono">
                  {[
                    pipeline.execution_config.platform_commands.android
                      ? `android: ${pipeline.execution_config.platform_commands.android}`
                      : '',
                    pipeline.execution_config.platform_commands.ios
                      ? `ios: ${pipeline.execution_config.platform_commands.ios}`
                      : '',
                    pipeline.execution_config.platform_commands.macos
                      ? `macos: ${pipeline.execution_config.platform_commands.macos}`
                      : '',
                  ]
                    .filter(Boolean)
                    .join(', ')}
                </span>
              </KV>
            ) : null}
            <KV label="Env vars">
              {(pipeline.execution_config.env?.length ?? 0) > 0
                ? `${pipeline.execution_config.env!.length} configured`
                : 'none'}
            </KV>
            <KV label="Artifact patterns">
              <span className="font-mono">
                {pipeline.execution_config.artifact_patterns.length > 0
                  ? pipeline.execution_config.artifact_patterns.join(', ')
                  : 'none'}
              </span>
            </KV>
          </Section>

          {pipeline.execution_config.platforms.includes('android') && (
            <Section title="Android signing">
              {signingQuery.data ? (
                <>
                  <KV label="Release">
                    {signingQuery.data.release.enabled
                      ? `enabled (${signingQuery.data.release.keystore_filename ?? 'keystore configured'})`
                      : 'disabled'}
                  </KV>
                  <KV label="Debug">
                    {signingQuery.data.debug.enabled
                      ? `enabled (${signingQuery.data.debug.keystore_filename ?? 'keystore configured'})`
                      : 'disabled'}
                  </KV>
                </>
              ) : (
                <p className="py-1 text-xs text-muted-foreground">
                  Not configured
                </p>
              )}
            </Section>
          )}

          {pipeline.execution_config.platforms.includes('ios') && (
            <Section title="iOS signing">
              {iosSigningQuery.data ? (
                <>
                  <KV label="Status">
                    {iosSigningQuery.data.enabled ? 'enabled' : 'disabled'}
                  </KV>
                  {iosSigningQuery.data.enabled && (
                    <>
                      <KV label="Mode">
                        {iosSigningQuery.data.mode === 'manual'
                          ? 'Manual (.p12 + provisioning profiles)'
                          : iosSigningQuery.data.mode === 'api'
                            ? 'API (App Store Connect)'
                            : 'Hybrid (manual cert + API automation)'}
                      </KV>
                      {iosSigningQuery.data.team_id && (
                        <KV label="Team ID">
                          <span className="font-mono">
                            {iosSigningQuery.data.team_id}
                          </span>
                        </KV>
                      )}
                      {iosSigningQuery.data.bundle_ids.length > 0 && (
                        <KV label="Bundle IDs">
                          <div className="flex flex-wrap gap-1">
                            {iosSigningQuery.data.bundle_ids.map((id) => (
                              <Badge
                                key={id}
                                variant="outline"
                                className="font-mono text-[11px]"
                              >
                                {id}
                              </Badge>
                            ))}
                          </div>
                        </KV>
                      )}
                      {(iosSigningQuery.data.mode === 'manual' ||
                        iosSigningQuery.data.mode === 'hybrid') && (
                        <KV label="Certificate">
                          {iosSigningQuery.data.has_p12
                            ? (iosSigningQuery.data.p12_filename ??
                              'configured')
                            : 'not uploaded'}
                        </KV>
                      )}
                      {(iosSigningQuery.data.mode === 'api' ||
                        iosSigningQuery.data.mode === 'hybrid') && (
                        <>
                          <KV label="API key">
                            {iosSigningQuery.data.has_api_key
                              ? `Key ${iosSigningQuery.data.api_key_id ?? 'configured'}`
                              : 'not configured'}
                          </KV>
                        </>
                      )}
                      {iosSigningQuery.data.provisioning_profiles.length >
                        0 && (
                        <KV label="Profiles">
                          {iosSigningQuery.data.provisioning_profiles.length}{' '}
                          provisioning profile
                          {iosSigningQuery.data.provisioning_profiles.length !==
                          1
                            ? 's'
                            : ''}
                        </KV>
                      )}
                    </>
                  )}
                </>
              ) : (
                <p className="py-1 text-xs text-muted-foreground">
                  Not configured
                </p>
              )}
            </Section>
          )}
        </CardContent>
      </Card>

      {/* Recent builds */}
      <Card>
        <CardContent>
          <h3 className="pb-3 text-sm font-medium">Recent builds</h3>
          {builds.length === 0 ? (
            <div className="space-y-2 py-3">
              <p className="text-sm text-muted-foreground">No builds yet.</p>
              {canTriggerBuild && projectHasSource ? (
                <Button size="sm" onClick={() => setTriggerBuildOpen(true)}>
                  <HugeiconsIcon icon={PlayIcon} size={14} />
                  Trigger first build
                </Button>
              ) : null}
            </div>
          ) : (
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
                {builds.map((build) => (
                  <TableRow
                    key={build.id}
                    className="group cursor-pointer"
                    onClick={() =>
                      void navigate({
                        to: '/builds/$buildId',
                        params: { buildId: build.id },
                      })
                    }
                  >
                    <TableCell className="font-mono text-sm group-hover:underline">
                      #{build.build_number}
                    </TableCell>
                    <TableCell>
                      <Badge variant={getStatusVariant(build.status)}>
                        {build.status}
                      </Badge>
                    </TableCell>
                    <TableCell className="font-mono text-xs text-muted-foreground">
                      {build.branch ?? 'n/a'}
                    </TableCell>
                    <TableCell className="text-sm text-muted-foreground">
                      {new Date(build.created_at * 1000).toLocaleString()}
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )}
        </CardContent>
      </Card>

      {/* Dialogs */}
      <TriggerBuildDialog
        open={triggerBuildOpen}
        onOpenChange={setTriggerBuildOpen}
        fixedProjectId={projectId}
        fixedPipelineId={pipeline.id}
        fixedPipelineName={pipeline.name}
        defaultBranch={projectData?.project.default_branch}
        description="Run this pipeline now with a branch or pinned commit."
        onBuildCreated={(buildId) => {
          void navigate({ to: '/builds/$buildId', params: { buildId } })
        }}
      />

      <AlertDialog open={deleteOpen} onOpenChange={setDeleteOpen}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Delete pipeline?</AlertDialogTitle>
            <AlertDialogDescription>
              This will permanently delete "{pipeline.name}". This action cannot
              be undone.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              onClick={handleDelete}
              className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
            >
              {deleteMutation.isPending ? 'Deleting...' : 'Delete'}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </PageLayout>
  )
}
