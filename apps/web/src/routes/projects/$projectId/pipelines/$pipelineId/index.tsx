import { lazy, Suspense, useEffect, useMemo, useState } from 'react'
import type { ReactElement, ReactNode } from 'react'
import {
  Link,
  createFileRoute,
  useLocation,
  useNavigate,
} from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  Alert02Icon,
  Delete02Icon,
  InformationCircleIcon,
  PlayIcon,
} from '@hugeicons/core-free-icons'
import { toast } from '@/lib/toast'
import type { Build, BuildPlatform, Pipeline } from '@oore/client/models'

import {
  getActiveInstanceOrRedirect,
  requireInstanceRoleOrRedirect,
} from '@/lib/instance-context'
import { useBuilds } from '@/hooks/use-builds'
import {
  canChangeApprovalPolicy,
  hasProjectPermission,
  useHasPermissions,
} from '@/hooks/use-permissions'
import { useAuthStore } from '@/stores/auth-store'
import {
  useDeletePipeline,
  usePipeline,
  usePipelineAndroidSigning,
  usePipelineIosSigning,
  usePipelineWorkflows,
  useRepositoryWorkflows,
  useUpdatePipeline,
} from '@/hooks/use-pipelines'
import {
  usePipelineWorkflowSettings,
  useSigningIdentities,
} from '@/hooks/use-signing-identities'
import { useSecrets } from '@/hooks/use-secrets'
import {
  pipelineSectionSummaries,
  pipelineWorkflowProblem,
} from '@/lib/pipeline-form-utils'
import {
  approvalInForce,
  pipelineFile,
  publishingInForce,
  repositoryFileUrl,
  signingInForce,
  sourceSummary,
  triggersInForce,
  usesPipeline,
  type SourcedValue,
} from '@/lib/pipeline-sources'
import { useProject } from '@/hooks/use-projects'
import {
  usePipelineAppStorePublish,
  usePipelineGooglePlayPublish,
} from '@/hooks/use-pipeline-publish'
import { CollectionError } from '@/components/collection'
import { enabledStatus } from '@/lib/status-variants'
import { BUILD_PLATFORM_LABELS } from '@/lib/build-platforms'
import { formatEnumLabel } from '@/lib/format-utils'
import { DangerZone } from '@/components/danger-zone'
import { RelativeTime } from '@/components/relative-time'
import { StatusBadge } from '@/components/status-badge'
import { ReleaseOutcomeBadge } from '@/components/release-outcome-badge'
import { PageMeta } from '@/lib/seo'
import {
  DataTable,
  useDataTable,
  type DataTableColumnDef,
} from '@/components/data-table'
import {
  Alert,
  AlertAction,
  AlertDescription,
  AlertTitle,
} from '@/components/ui/alert'
import {
  pipelineHealthBanner,
  pipelineProblemBanner,
  type PipelineBanner,
} from '@/lib/repository-config-files'
import ConfirmDialog from '@/components/ConfirmDialog'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent } from '@/components/ui/card'
import { LatestInstallLinksCard } from '@/components/latest-install-links-card'
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from '@/components/ui/empty'
import { appStoreDestination, playDestination } from '@/lib/store-destination'
import PageHeader from '@/components/page-header'
import PageLayout from '@/components/page-layout'
import {
  BuildNumberValue,
  BuildSourceValue,
  EmptyValue,
} from '@/components/table-values'
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { WorkflowBadge } from '@/components/workflow-badge'
import { Skeleton } from '@/components/ui/skeleton'
import { searchChoice, searchString } from '@/lib/search-input'
import type { SearchInput } from '@/lib/search-input'
import { RepositoryFileUrl } from '@/components/pipeline-page/source-label'
import { Mono } from '@/components/layout/mono'
import { Row } from '@/components/layout/row'
import { Stack } from '@/components/layout/stack'
import { textVariants } from '@/lib/recipes'
import { cn } from '@/lib/utils'
import {
  PIPELINE_ROWS,
  PipelineRow,
  PipelineRowList,
  rowForAnchor,
  type PipelineRowKey,
} from './-pipeline-row'
import {
  BuildRowContent,
  PublishingRowContent,
  SecretsRowContent,
  SigningRowContent,
  SourceRowContent,
  pipelineFormValues,
  pipelineSigningValue,
  type PipelineRowContext,
} from './-pipeline-settings'

const recentBuildColumns: Array<DataTableColumnDef<Build>> = [
  {
    accessorKey: 'build_number',
    header: 'Build',
    cell: ({ row }) => <BuildNumberValue build={row.original} />,
    enableSorting: false,
  },
  {
    accessorKey: 'status',
    header: 'Status',
    cell: ({ row }) => (
      <div className="flex flex-wrap items-center gap-1">
        <StatusBadge kind="build" value={row.original.status} />
        <ReleaseOutcomeBadge build={row.original} />
      </div>
    ),
    enableSorting: false,
  },
  {
    accessorKey: 'branch',
    header: 'Source',
    cell: ({ row }) => (
      <BuildSourceValue
        branch={row.original.branch}
        commitSha={row.original.commit_sha}
        context={row.original.context}
        title={row.original.commit_title}
        author={row.original.commit_author}
      />
    ),
    enableSorting: false,
  },
  {
    accessorKey: 'created_at',
    header: 'Created',
    cell: ({ row }) => <RelativeTime at={row.original.created_at} />,
    enableSorting: false,
  },
]

const RECENT_BUILD_LIMIT = 20

const EVENT_PHRASES = new Map([
  ['push', 'pushes'],
  ['pull_request', 'pull requests'],
  ['tag_push', 'tags'],
  ['merge_request', 'merge requests'],
])

function joinPhrases(items: Array<string>) {
  if (items.length <= 1) return items.join('')
  return `${items.slice(0, -1).join(', ')} and ${items.at(-1)}`
}

/** "Builds Android and iOS on pushes to main." as the page subtitle. */
function describePipeline(pipeline: Pipeline, manualOnlyTriggers: boolean) {
  const platforms = joinPhrases(
    pipeline.execution_config.platforms.map((p) => BUILD_PLATFORM_LABELS[p]),
  )
  const subject = platforms
    ? `Builds ${platforms}`
    : 'Runs its configured build'
  const { branches, events } = pipeline.trigger_config
  if (manualOnlyTriggers) return `${subject} when you run it.`
  const when =
    events.length === 0
      ? 'on every repository event'
      : `on ${joinPhrases(events.map((event) => EVENT_PHRASES.get(event) ?? formatEnumLabel(event).toLowerCase()))}`
  const where = branches.length > 0 ? ` to ${branches.join(', ')}` : ''
  return `${subject} ${when}${where}.`
}

const workflowColumn: DataTableColumnDef<Build> = {
  accessorKey: 'workflow',
  header: 'Workflow',
  cell: ({ row }) =>
    row.original.workflow ? (
      <WorkflowBadge workflow={row.original.workflow} />
    ) : (
      <EmptyValue />
    ),
  enableSorting: false,
}

// Base UI selects treat an empty value as unselected.
const ALL_WORKFLOWS = '__all__'

/** Filters Recent builds to one workflow of the pipeline's file. */
function WorkflowFilter({
  workflows,
  value,
  onChange,
}: {
  workflows: Array<string>
  value?: string
  onChange: (workflow?: string) => void
}) {
  // Keep a filter from the URL visible even when the file dropped it.
  const names =
    value && !workflows.includes(value) ? [...workflows, value] : workflows
  // The trigger shows "Workflow: release"; the list shows plain names.
  const items = {
    [ALL_WORKFLOWS]: 'All workflows',
    ...Object.fromEntries(names.map((name) => [name, `Workflow: ${name}`])),
  }
  return (
    <Select
      value={value ?? ALL_WORKFLOWS}
      onValueChange={(next) =>
        onChange(next && next !== ALL_WORKFLOWS ? next : undefined)
      }
      items={items}
    >
      <SelectTrigger size="sm" aria-label="Filter by workflow">
        <SelectValue />
      </SelectTrigger>
      <SelectContent align="end">
        <SelectGroup>
          <SelectItem value={ALL_WORKFLOWS}>All workflows</SelectItem>
          {names.map((name) => (
            <SelectItem key={name} value={name}>
              {name}
            </SelectItem>
          ))}
        </SelectGroup>
      </SelectContent>
    </Select>
  )
}

function RecentBuildsTable({ builds }: { builds: Array<Build> }) {
  // A pipeline whose builds never named a workflow has no column for it.
  const columns = useMemo(
    () =>
      builds.some((build) => build.workflow)
        ? [
            ...recentBuildColumns.slice(0, 2),
            workflowColumn,
            ...recentBuildColumns.slice(2),
          ]
        : recentBuildColumns,
    [builds],
  )
  const table = useDataTable({
    columns,
    data: builds,
    getRowId: (build) => build.id,
  })
  return <DataTable table={table} />
}

const loadTriggerBuildDrawer = () => import('@/components/trigger-build-drawer')
const TriggerBuildDrawer = lazy(loadTriggerBuildDrawer)

/** The pipeline's own trigger settings, such as `Pushes to main`. */
function describeTriggers(pipeline: Pipeline, manualOnlyTriggers: boolean) {
  const schedules = (pipeline.trigger_config.schedules ?? []).length
  const scheduled =
    schedules > 0
      ? `${schedules} ${schedules === 1 ? 'schedule' : 'schedules'}`
      : ''
  if (manualOnlyTriggers) return scheduled || 'Only when run by hand'
  const { branches, events } = pipeline.trigger_config
  const when =
    events.length === 0
      ? 'Every repository event'
      : joinPhrases(
          events.map(
            (event) =>
              EVENT_PHRASES.get(event) ?? formatEnumLabel(event).toLowerCase(),
          ),
        )
  const where = branches.length > 0 ? ` to ${branches.join(', ')}` : ''
  const sentence = `${when.charAt(0).toUpperCase()}${when.slice(1)}${where}`
  return scheduled ? `${sentence}, ${scheduled}` : sentence
}

/** Distinct values of a row, such as `coin-upload, Not signed`. */
function values(entries: Array<SourcedValue>) {
  return [...new Set(entries.map((entry) => entry.value))].join(', ')
}

/** Old tab names, so links to `?tab=publishing` still open the right row. */
const LEGACY_TABS = ['overview', 'workflows', 'publishing', 'secrets'] as const

interface PipelineDetailSearch {
  /** The row to open. */
  row?: PipelineRowKey
  /** @deprecated The page had tabs; `?tab=publishing` opens that row. */
  tab?: (typeof LEGACY_TABS)[number]
  /** Show only builds of this workflow in Recent builds. */
  workflow?: string
}

function validatePipelineSearch(search: SearchInput): PipelineDetailSearch {
  const row = searchChoice(search, 'row', PIPELINE_ROWS)
  const tab = searchChoice(search, 'tab', LEGACY_TABS)
  const workflow = searchString(search, 'workflow')?.trim()
  return {
    row: row ?? rowForAnchor(tab),
    workflow: workflow || undefined,
  }
}

export const Route = createFileRoute(
  '/projects/$projectId/pipelines/$pipelineId/',
)({
  validateSearch: validatePipelineSearch,
  beforeLoad: (context) => {
    const instance = getActiveInstanceOrRedirect()
    requireInstanceRoleOrRedirect(
      instance.id,
      ['owner', 'admin', 'developer'],
      context,
    )
  },
  component: PipelineDetailPage,
})

/**
 * The pipeline's one banner about its config file: the daemon's health from
 * its last read of the default branch, or, before that read, a live look
 * at the file. Absent when the file is fine.
 */
function PipelineConfigAlert({
  banner,
  checked,
  editLink,
}: {
  banner: PipelineBanner
  checked?: { at: number; commit: string }
  editLink?: ReactElement
}) {
  return (
    <Alert
      variant={banner.variant}
      role={banner.variant === 'destructive' ? 'alert' : 'status'}
    >
      <HugeiconsIcon
        icon={
          banner.variant === 'destructive' ? Alert02Icon : InformationCircleIcon
        }
        size={16}
      />
      <AlertTitle>{banner.title}</AlertTitle>
      <AlertDescription>
        {checked ? (
          <>
            {banner.description.replace(/\.?$/, '. ')}Checked{' '}
            <RelativeTime at={checked.at} /> at{' '}
            <Mono>{checked.commit.slice(0, 7)}</Mono>.
          </>
        ) : (
          banner.description
        )}
      </AlertDescription>
      {editLink ? (
        <AlertAction>
          <Button
            variant="outline"
            size="sm"
            render={editLink}
            nativeButton={false}
          >
            Edit
          </Button>
        </AlertAction>
      ) : null}
    </Alert>
  )
}

/**
 * Which rows are open. A link with `?row=` or an old Edit page anchor
 * opens its row and scrolls to it; the person opens and closes the rest.
 */
function useOpenRows(row: PipelineRowKey | undefined) {
  const hash = useLocation({ select: (location) => location.hash })
  const target = row ?? rowForAnchor(hash)
  // What the person opened or closed; a row they did not touch is open
  // when a link pointed at it.
  const [choices, setChoices] = useState<ReadonlyMap<PipelineRowKey, boolean>>(
    () => new Map(),
  )
  const [seenTarget, setSeenTarget] = useState(target)
  if (target !== seenTarget) {
    // A new link on this page opens its row again, even if it was closed.
    setSeenTarget(target)
    if (target) setChoices((current) => new Map(current).set(target, true))
  }
  useEffect(() => {
    if (!target) return
    // Two frames: the row's content mounts before anything in it is found.
    requestAnimationFrame(() =>
      requestAnimationFrame(() => {
        const element =
          (hash ? document.getElementById(hash) : null) ??
          document.getElementById(`row-${target}`)
        element?.scrollIntoView({ block: 'start', behavior: 'smooth' })
      }),
    )
  }, [target, hash])
  const isOpen = (key: PipelineRowKey) => choices.get(key) ?? key === target
  function toggle(key: PipelineRowKey, next: boolean) {
    setChoices((current) => new Map(current).set(key, next))
  }
  return [isOpen, toggle] as const
}

/* ------------------------------------------------------------------ */
/*  Main page                                                          */
/* ------------------------------------------------------------------ */

function PipelineDetailPage() {
  const { projectId, pipelineId } = Route.useParams()
  const navigate = useNavigate()
  const { row, workflow: workflowFilter } = Route.useSearch()
  const [isRowOpen, toggleRow] = useOpenRows(row)
  const { data, isLoading, error } = usePipeline(pipelineId)
  const [
    canWriteGlobally,
    canTriggerBuildGlobally,
    canReadInstanceSettings,
    canShareGlobally,
  ] = useHasPermissions([
    'pipelines:write',
    'builds:write',
    'instance_settings:read',
    'artifacts:write',
  ])
  const { data: projectData } = useProject(projectId)
  const projectRole = projectData?.project.current_user_role
  const instanceRole = useAuthStore((state) => state.user?.role)
  const canWrite =
    canWriteGlobally && hasProjectPermission(projectRole, 'pipelines:write')
  const canDelete = hasProjectPermission(projectRole, 'pipelines:delete')
  const canShareBuilds =
    canShareGlobally && hasProjectPermission(projectRole, 'artifacts:write')
  const canTriggerBuild =
    canTriggerBuildGlobally && hasProjectPermission(projectRole, 'builds:write')
  const signingQuery = usePipelineAndroidSigning(pipelineId, {
    enabled: canWrite,
  })
  const iosSigningQuery = usePipelineIosSigning(pipelineId, {
    enabled: canWrite,
  })
  const buildsQuery = useBuilds({
    pipeline_id: pipelineId,
    workflow: workflowFilter,
    limit: RECENT_BUILD_LIMIT,
  })
  const pipelineWorkflowsQuery = usePipelineWorkflows(pipelineId)
  const workflowSettingsQuery = usePipelineWorkflowSettings(pipelineId)
  const identitiesQuery = useSigningIdentities(projectId)
  const secretsQuery = useSecrets({ scope: 'pipeline', pipelineId })
  const listing = pipelineWorkflowsQuery.data
  const file = pipelineFile(listing)
  const fileWorkflows = file?.workflows.map((workflow) => workflow.name) ?? []
  function setWorkflowFilter(workflow?: string) {
    void navigate({
      to: '/projects/$projectId/pipelines/$pipelineId',
      params: { projectId, pipelineId },
      search: { row, workflow },
      replace: true,
    })
  }
  const buildsAndroidForQueries =
    data?.pipeline.execution_config.platforms.includes('android') ?? false
  const buildsIosForQueries =
    data?.pipeline.execution_config.platforms.includes('ios') ?? false
  const googlePlayQuery = usePipelineGooglePlayPublish(pipelineId, {
    enabled: buildsAndroidForQueries,
  })
  const appStoreQuery = usePipelineAppStorePublish(pipelineId, {
    enabled: buildsIosForQueries,
  })
  // The daemon's health covers the pipeline's file once it has read the
  // default branch. Until then, look at the file live; discovery needs
  // pipeline write access.
  const repositoryFileQuery = useRepositoryWorkflows(
    projectId,
    data && data.pipeline.config_path.trim() !== '.oore.yaml'
      ? { path: data.pipeline.config_path }
      : undefined,
    {
      enabled:
        canWrite &&
        !!data &&
        !data.pipeline.health &&
        !!projectData?.project.repository_id,
    },
  )
  const updateMutation = useUpdatePipeline()
  const deleteMutation = useDeletePipeline()

  const [deleteOpen, setDeleteOpen] = useState(false)
  const [disableOpen, setDisableOpen] = useState(false)
  const [buildDrawerOpen, setBuildDrawerOpen] = useState(false)

  const builds = buildsQuery.data?.builds ?? []
  const project = projectData?.project
  // A build's context knows a self-managed GitLab host; the project does not.
  const repositoryHost = builds.find(
    (build) => build.context?.repository_host_url,
  )?.context?.repository_host_url
  const fileUrl = useMemo(
    () => (path: string) =>
      repositoryFileUrl({
        fullName: project?.repository_full_name,
        host: repositoryHost,
        path,
        provider: project?.repository_provider,
        reference: listing?.reference ?? project?.default_branch,
      }),
    [
      project?.repository_full_name,
      project?.repository_provider,
      project?.default_branch,
      repositoryHost,
      listing?.reference,
    ],
  )

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
  // What builds: the file's workflows when it has them, else the pipeline.
  const builtPlatforms: Array<BuildPlatform> = [
    ...new Set([
      ...pipeline.execution_config.platforms,
      ...(file?.workflows.flatMap((workflow) => workflow.platforms) ?? []),
    ]),
  ]
  const storePlatforms = builtPlatforms.filter(
    (platform): platform is 'android' | 'ios' =>
      platform === 'android' || platform === 'ios',
  )
  // Unknown until the project loads; only a loaded project without a
  // repository blocks builds.
  const projectMissingSource = !!project && !project.repository_id
  const projectHasSource = !!project?.repository_id
  const manualOnlyTriggers = project?.repository_provider === 'local_git'
  const canChangeApproval = canChangeApprovalPolicy(instanceRole, projectRole)

  const workflowProblem =
    !pipeline.health && repositoryFileQuery.data
      ? pipelineWorkflowProblem(repositoryFileQuery.data.workflows, pipeline)
      : undefined
  const configBanner = pipeline.health
    ? pipelineHealthBanner(pipeline.health, pipeline)
    : workflowProblem && repositoryFileQuery.data
      ? pipelineProblemBanner(
          workflowProblem,
          repositoryFileQuery.data.reference,
        )
      : null

  /* What is in force, and where each value comes from. */
  const workflowSettings = workflowSettingsQuery.data ?? []
  const identityNames = new Map(
    (identitiesQuery.data ?? []).map((identity) => [
      identity.id,
      identity.name,
    ]),
  )
  const identityName = (id: string) =>
    identityNames.get(id) ?? 'Deleted identity'
  const triggers = triggersInForce({
    file,
    pipelineValue: describeTriggers(pipeline, manualOnlyTriggers),
  })
  const approval = approvalInForce({
    file,
    pipelineValue: pipeline.approval.required ? 'Required' : 'Not required',
    settings: workflowSettings,
  })
  const signingKnown = !!signingQuery.data && !!iosSigningQuery.data
  const signing = new Map(
    storePlatforms.map((platform) => [
      platform,
      signingInForce({
        file,
        identityName,
        pipelineValue: pipelineSigningValue(
          platform,
          signingQuery.data,
          iosSigningQuery.data,
        ),
        platform,
        settings: workflowSettings,
      }),
    ]),
  )
  const publishing = new Map(
    storePlatforms.map((platform) => {
      const query = platform === 'android' ? googlePlayQuery : appStoreQuery
      const config =
        platform === 'android'
          ? googlePlayQuery.data?.config
          : appStoreQuery.data?.config
      const pipelineValue = !query.data
        ? '…'
        : config?.enabled
          ? `Uploads to ${platform === 'android' && googlePlayQuery.data?.config ? playDestination(googlePlayQuery.data.config) : appStoreQuery.data ? appStoreDestination(appStoreQuery.data.config) : ''}`
          : 'Off'
      return [
        platform,
        publishingInForce({
          file,
          pipelineValue,
          platform,
          settings: workflowSettings,
        }),
      ]
    }),
  )
  const storeNames = {
    android: 'Google Play',
    ios: 'App Store Connect',
  } as const
  const formSummaries = pipelineSectionSummaries(
    pipelineFormValues(pipeline, undefined, undefined, manualOnlyTriggers),
    manualOnlyTriggers,
  )
  const showWorkflows =
    pipelineWorkflowsQuery.isError ||
    !!listing?.default_workflow ||
    fileWorkflows.length > 0 ||
    workflowSettings.length > 0 ||
    listing?.state === 'invalid'
  const secretCount = secretsQuery.data?.length

  const context: PipelineRowContext = {
    pipeline,
    projectId,
    canWrite,
    canChangeApproval,
    manualOnlyTriggers,
    file,
    listing,
    defaultBranch: project?.default_branch,
    hasRepository: projectHasSource,
    androidSigning: signingQuery.data,
    iosSigning: iosSigningQuery.data,
  }

  const rowSummaries = {
    source: {
      summary: `${pipeline.config_path} · ${values(triggers)}`,
      sources: [
        ...new Set([...sourceSummary([...triggers, ...approval]), 'set here']),
      ],
    },
    build: file
      ? {
          summary: `Builds ${joinPhrases(builtPlatforms.map((platform) => BUILD_PLATFORM_LABELS[platform]))}`,
          sources: [`from ${file.path}`],
        }
      : {
          summary: `${joinPhrases(builtPlatforms.map((platform) => BUILD_PLATFORM_LABELS[platform]))}, ${formSummaries.build.charAt(0).toLowerCase()}${formSummaries.build.slice(1)}`,
          sources: ['set here'],
        },
    signing: {
      summary:
        storePlatforms.length === 0
          ? 'Nothing to sign'
          : !canWrite
            ? 'Visible to people who can edit the pipeline'
            : storePlatforms
                .map((platform) =>
                  signingKnown || file
                    ? `${BUILD_PLATFORM_LABELS[platform]}: ${values(signing.get(platform) ?? [])}`
                    : BUILD_PLATFORM_LABELS[platform],
                )
                .join(' · '),
      sources: sourceSummary(
        storePlatforms.flatMap((platform) => signing.get(platform) ?? []),
      ),
    },
    publishing: {
      summary:
        storePlatforms.length === 0
          ? 'Nothing to publish'
          : storePlatforms
              .map(
                (platform) =>
                  `${storeNames[platform]}: ${values(publishing.get(platform) ?? [])}`,
              )
              .join(' · '),
      sources: sourceSummary(
        storePlatforms.flatMap((platform) => publishing.get(platform) ?? []),
      ),
    },
    secrets: {
      summary:
        secretCount === undefined
          ? 'Pipeline secrets'
          : secretCount === 0
            ? 'No pipeline secrets'
            : `${secretCount} ${secretCount === 1 ? 'secret' : 'secrets'}`,
      sources: ['set here'],
    },
  } satisfies Record<
    PipelineRowKey,
    { summary: ReactNode; sources: Array<string> }
  >

  function handleToggleEnabled() {
    updateMutation.mutate(
      { pipelineId: pipeline.id, data: { enabled: !pipeline.enabled } },
      {
        onSettled: () => setDisableOpen(false),
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
    deleteMutation.mutate(
      { pipelineId, projectId },
      {
        onSuccess: () => {
          toast.success('Pipeline deleted')
          void navigate({ to: '/projects/$projectId', params: { projectId } })
        },
        onError: (err) =>
          toast.error(`Failed to delete pipeline: ${err.message}`),
      },
    )
  }

  const renderRow = (
    key: PipelineRowKey,
    title: string,
    content: () => ReactNode,
  ) => (
    <PipelineRow
      key={key}
      id={key}
      open={isRowOpen(key)}
      onOpenChange={(next) => toggleRow(key, next)}
      sources={rowSummaries[key].sources}
      summary={rowSummaries[key].summary}
      title={title}
    >
      {isRowOpen(key) ? content() : null}
    </PipelineRow>
  )

  return (
    <PageLayout width="wide">
      <PageMeta title={label} noindex />
      <PageHeader
        title={pipeline.name}
        description={describePipeline(pipeline, manualOnlyTriggers)}
        meta={
          <>
            <StatusBadge
              kind="enabled"
              value={enabledStatus(pipeline.enabled)}
            />
            {builtPlatforms.map((p) => (
              <Badge key={p} variant="outline" className="text-[11px]">
                {BUILD_PLATFORM_LABELS[p]}
              </Badge>
            ))}
            <span>
              Updated <RelativeTime at={pipeline.updated_at} />
            </span>
          </>
        }
        actions={
          canWrite || canTriggerBuild ? (
            <>
              {canTriggerBuild ? (
                <Suspense
                  fallback={
                    <Button disabled>
                      <HugeiconsIcon icon={PlayIcon} />
                      Run build
                    </Button>
                  }
                >
                  <TriggerBuildDrawer
                    fixedProjectId={projectId}
                    fixedPipelineId={pipeline.id}
                    fixedPipelineName={pipeline.name}
                    defaultBranch={project?.default_branch ?? undefined}
                    description="Run this pipeline now with a branch or pinned commit."
                    open={buildDrawerOpen}
                    onOpenChange={setBuildDrawerOpen}
                    onBuildCreated={(buildId) => {
                      void navigate({
                        to: '/builds/$buildId',
                        params: { buildId },
                      })
                    }}
                  >
                    <Button
                      variant={pipeline.enabled ? 'default' : 'outline'}
                      disabled={projectMissingSource}
                    >
                      <HugeiconsIcon icon={PlayIcon} />
                      Run build
                    </Button>
                  </TriggerBuildDrawer>
                </Suspense>
              ) : null}
              {canWrite ? (
                <Button
                  variant={pipeline.enabled ? 'outline' : 'default'}
                  onClick={() =>
                    pipeline.enabled
                      ? setDisableOpen(true)
                      : handleToggleEnabled()
                  }
                  disabled={updateMutation.isPending}
                >
                  {pipeline.enabled ? 'Disable' : 'Enable'}
                </Button>
              ) : null}
            </>
          ) : undefined
        }
      />
      {!pipeline.enabled ? (
        <Alert role="status">
          <HugeiconsIcon icon={InformationCircleIcon} size={16} />
          <AlertDescription>
            This pipeline is disabled. Pushes, pull requests and schedules do
            not start builds until you enable it.
          </AlertDescription>
        </Alert>
      ) : null}
      {configBanner ? (
        <PipelineConfigAlert
          banner={configBanner}
          checked={
            pipeline.health
              ? {
                  at: pipeline.health.checked_at,
                  commit: pipeline.health.commit_sha,
                }
              : undefined
          }
          editLink={
            canWrite ? (
              <Link
                to="/projects/$projectId/pipelines/$pipelineId"
                params={{ projectId, pipelineId }}
                search={{ row: 'source' }}
                hash="basics"
              />
            ) : undefined
          }
        />
      ) : null}
      {projectMissingSource ? (
        <Alert variant="destructive">
          <HugeiconsIcon icon={InformationCircleIcon} size={16} />
          <AlertDescription>
            This app has no linked source repository. Link a repository before
            triggering builds.
          </AlertDescription>
        </Alert>
      ) : null}

      <RepositoryFileUrl.Provider value={fileUrl}>
        <PipelineRowList>
          {renderRow('source', 'Source and triggers', () => (
            <SourceRowContent
              approval={approval}
              context={context}
              showWorkflows={showWorkflows}
              triggers={triggers}
            />
          ))}
          {renderRow('build', 'Build', () => (
            <BuildRowContent context={context} />
          ))}
          {renderRow('signing', 'Signing', () => (
            <SigningRowContent
              context={context}
              platforms={storePlatforms}
              signing={Object.fromEntries(signing)}
              signingError={signingQuery.error ?? iosSigningQuery.error}
              onRetry={() => {
                if (signingQuery.isError) void signingQuery.refetch()
                if (iosSigningQuery.isError) void iosSigningQuery.refetch()
              }}
            />
          ))}
          {renderRow('publishing', 'Publishing', () => (
            <PublishingRowContent
              androidSignedElsewhere={
                (signing.get('android') ?? []).length > 0 &&
                !usesPipeline(signing.get('android') ?? [])
              }
              canReadInstanceSecrets={canReadInstanceSettings}
              context={context}
              overrides={storePlatforms.map((platform) => ({
                label: storeNames[platform],
                entries: (publishing.get(platform) ?? []).filter(
                  (entry) => entry.source.kind === 'workflow',
                ),
              }))}
              platforms={storePlatforms}
            />
          ))}
          {renderRow('secrets', 'Secrets', () => (
            <SecretsRowContent
              canReadInstanceSecrets={canReadInstanceSettings}
              context={context}
            />
          ))}
        </PipelineRowList>
      </RepositoryFileUrl.Provider>

      <LatestInstallLinksCard
        pipelineId={pipelineId}
        platforms={builtPlatforms.filter((platform) => platform !== 'macos')}
        canWrite={canShareBuilds}
      />
      {/* Recent builds */}
      <Card>
        <CardContent>
          <Row gap={3} className="justify-between pb-3">
            <h3 className="text-sm font-medium">Recent builds</h3>
            {fileWorkflows.length > 1 || workflowFilter ? (
              <WorkflowFilter
                workflows={fileWorkflows}
                value={workflowFilter}
                onChange={setWorkflowFilter}
              />
            ) : null}
          </Row>
          {buildsQuery.error ? (
            <CollectionError
              title="Failed to load recent builds"
              description={buildsQuery.error.message}
              onRetry={() => void buildsQuery.refetch()}
            />
          ) : buildsQuery.isLoading ? (
            <Skeleton className="h-40 w-full" />
          ) : builds.length === 0 && workflowFilter ? (
            <Empty className="p-8">
              <EmptyHeader>
                <EmptyTitle>No {workflowFilter} builds</EmptyTitle>
                <EmptyDescription>
                  This pipeline has not built the {workflowFilter} workflow yet.
                </EmptyDescription>
              </EmptyHeader>
              <EmptyContent>
                <Button
                  size="sm"
                  variant="outline"
                  onClick={() => setWorkflowFilter(undefined)}
                >
                  Show all workflows
                </Button>
              </EmptyContent>
            </Empty>
          ) : builds.length === 0 ? (
            <Empty className="p-8">
              <EmptyHeader>
                <EmptyMedia variant="icon">
                  <HugeiconsIcon icon={PlayIcon} />
                </EmptyMedia>
                <EmptyTitle>No builds yet</EmptyTitle>
                <EmptyDescription>
                  Run this pipeline to see its status, output, and artifacts
                  here.
                </EmptyDescription>
              </EmptyHeader>
              {canTriggerBuild && projectHasSource ? (
                <EmptyContent>
                  <Button size="sm" onClick={() => setBuildDrawerOpen(true)}>
                    <HugeiconsIcon icon={PlayIcon} />
                    Run first build
                  </Button>
                </EmptyContent>
              ) : null}
            </Empty>
          ) : (
            <Stack gap={3}>
              <RecentBuildsTable builds={builds} />
              {(buildsQuery.data?.total ?? builds.length) > builds.length ? (
                <Row
                  gap={3}
                  className={cn(
                    textVariants({ variant: 'muted' }),
                    'justify-between',
                  )}
                >
                  <span>Showing the {builds.length} most recent builds.</span>
                  <Button
                    variant="outline"
                    size="sm"
                    nativeButton={false}
                    render={
                      <Link
                        to="/projects/$projectId"
                        params={{ projectId }}
                        search={{ tab: 'builds', pipeline: pipelineId }}
                      />
                    }
                  >
                    View all builds
                  </Button>
                </Row>
              ) : null}
            </Stack>
          )}
        </CardContent>
      </Card>
      {canDelete ? (
        <DangerZone
          surface="card"
          title="Delete this pipeline"
          description={`Permanently removes "${pipeline.name}" and its configuration from this app. This cannot be undone.`}
          action={
            <Button variant="destructive" onClick={() => setDeleteOpen(true)}>
              <HugeiconsIcon icon={Delete02Icon} />
              Delete pipeline
            </Button>
          }
        />
      ) : null}

      <ConfirmDialog
        open={disableOpen}
        onOpenChange={setDisableOpen}
        title={`Disable ${pipeline.name}?`}
        description="Pushes, pull requests and schedules stop starting builds for this pipeline. Builds already running finish. You can enable it again at any time."
        confirmLabel="Disable pipeline"
        isPending={updateMutation.isPending}
        pendingLabel="Disabling..."
        onConfirm={handleToggleEnabled}
      />

      <ConfirmDialog
        open={deleteOpen}
        onOpenChange={setDeleteOpen}
        title="Delete pipeline?"
        description={`This permanently deletes "${pipeline.name}" with its build history, schedules, signing files, secrets and publishing settings. It cannot be undone.`}
        confirmLabel="Delete pipeline"
        confirmVariant="destructive"
        isPending={deleteMutation.isPending}
        pendingLabel="Deleting..."
        onConfirm={handleDelete}
      />
    </PageLayout>
  )
}
