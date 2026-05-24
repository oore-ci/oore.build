import { Link, createFileRoute, useSearch } from '@tanstack/react-router'
import { useMemo, useState } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  Add01Icon,
  InformationCircleIcon,
  PlayIcon,
} from '@hugeicons/core-free-icons'

import {
  getActiveInstanceOrRedirect,
  requireAuthOrRedirect,
} from '@/lib/instance-context'
import { useBuilds } from '@/hooks/use-builds'
import { useHasPermission } from '@/hooks/use-permissions'
import { useProjects } from '@/hooks/use-projects'
import { useSetupStatus } from '@/hooks/use-setup'
import { getStatusVariant } from '@/lib/status-variants'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import PageHeader from '@/components/page-header'
import PageLayout from '@/components/page-layout'
import SetupHint from '@/components/setup-hint'
import { Skeleton } from '@/components/ui/skeleton'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import {
  Pagination,
  PaginationContent,
  PaginationItem,
  PaginationLink,
  PaginationNext,
  PaginationPrevious,
} from '@/components/ui/pagination'
import { relativeTime } from '@/lib/format-utils'
import { PageMeta } from '@/lib/seo'
import TriggerBuildDialog from '@/components/trigger-build-dialog'

const PAGE_SIZE = 20

const STATUS_OPTIONS: Record<string, string> = {
  all: 'All statuses',
  queued: 'Queued',
  running: 'Running',
  succeeded: 'Succeeded',
  failed: 'Failed',
  canceled: 'Canceled',
}

export const Route = createFileRoute('/builds/')({
  staticData: { breadcrumbLabel: 'Builds' },
  validateSearch: (search: Record<string, unknown>): { page?: number } => ({
    page: Number(search.page) > 1 ? Number(search.page) : undefined,
  }),
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireAuthOrRedirect(instance.id)
  },
  component: BuildsListPage,
})

function BuildsListPage() {
  const navigate = Route.useNavigate()
  const search = useSearch({ from: '/builds/' })
  const page = search.page ?? 1
  const offset = (page - 1) * PAGE_SIZE

  const [projectFilter, setProjectFilter] = useState<string>('all')
  const [statusFilter, setStatusFilter] = useState<string>('all')
  const [branchFilter, setBranchFilter] = useState('')
  const buildsQuery = useBuilds({
    limit: PAGE_SIZE,
    offset,
    project_id: projectFilter !== 'all' ? projectFilter : undefined,
    status: statusFilter !== 'all' ? statusFilter : undefined,
    branch: branchFilter.trim() || undefined,
  })
  const total = buildsQuery.data?.total ?? 0
  const totalPages = Math.max(1, Math.ceil(total / PAGE_SIZE))
  const projectsQuery = useProjects({ limit: 200 })
  const setupStatusQuery = useSetupStatus()
  const canTriggerBuild = useHasPermission('builds', 'write')
  const canWriteProjects = useHasPermission('projects', 'write')
  const canWriteIntegrations = useHasPermission('integrations', 'write')
  const [triggerBuildOpen, setTriggerBuildOpen] = useState(false)

  const builds = useMemo(
    () => buildsQuery.data?.builds ?? [],
    [buildsQuery.data?.builds],
  )
  const projects = useMemo(
    () => projectsQuery.data?.projects ?? [],
    [projectsQuery.data?.projects],
  )
  const runtimeMode = setupStatusQuery.data?.runtime_mode ?? 'local'
  const integrationConnectTo = '/settings/integrations'
  const missingProjects =
    !projectsQuery.isLoading && !projectsQuery.error && projects.length === 0
  const isLoading = buildsQuery.isLoading || projectsQuery.isLoading
  const error = buildsQuery.error ?? projectsQuery.error

  return (
    <PageLayout width="wide">
      <PageMeta title="Builds" noindex />
      <PageHeader
        title="Builds"
        description="Queue, execution, and historical run inventory across projects."
        actions={
          !missingProjects && canTriggerBuild ? (
            <Button onClick={() => setTriggerBuildOpen(true)}>
              <HugeiconsIcon icon={PlayIcon} size={16} />
              Run Build
            </Button>
          ) : undefined
        }
      />

      {!missingProjects && !isLoading ? (
        <div className="flex items-center gap-3">
          <Select
            value={projectFilter}
            onValueChange={(v) => setProjectFilter(v ?? 'all')}
            items={Object.fromEntries([
              ['all', 'All projects'],
              ...projects.map((p) => [p.id, p.name] as const),
            ])}
          >
            <SelectTrigger className="w-44">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="all">All projects</SelectItem>
              {projects.map((p) => (
                <SelectItem key={p.id} value={p.id}>
                  {p.name}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <Select
            value={statusFilter}
            onValueChange={(v) => setStatusFilter(v ?? 'all')}
            items={STATUS_OPTIONS}
          >
            <SelectTrigger className="w-36">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {Object.entries(STATUS_OPTIONS).map(([key, label]) => (
                <SelectItem key={key} value={key}>
                  {label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <Input
            placeholder="Filter by branch..."
            value={branchFilter}
            onChange={(e) => setBranchFilter(e.target.value)}
            className="max-w-xs"
          />
          {projectFilter !== 'all' || statusFilter !== 'all' || branchFilter ? (
            <Button
              variant="ghost"
              size="sm"
              onClick={() => {
                setProjectFilter('all')
                setStatusFilter('all')
                setBranchFilter('')
              }}
            >
              Clear filters
            </Button>
          ) : null}
        </div>
      ) : null}

      {isLoading ? (
        <Card>
          <CardContent className="space-y-3">
            <Skeleton className="h-10 w-full" />
            <Skeleton className="h-10 w-full" />
            <Skeleton className="h-10 w-full" />
          </CardContent>
        </Card>
      ) : null}

      {error ? (
        <Alert variant="destructive">
          <HugeiconsIcon icon={InformationCircleIcon} size={16} />
          <AlertDescription>
            Failed to load builds: {error.message}
          </AlertDescription>
        </Alert>
      ) : null}

      {!isLoading && !error ? (
        missingProjects ? (
          <Card>
            <CardHeader>
              <CardTitle className="text-sm font-medium uppercase tracking-wider text-muted-foreground">
                Create Project First
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-4">
              <p className="text-sm text-muted-foreground">
                {runtimeMode === 'local'
                  ? 'Builds run through pipelines under projects. Create your first project from a local Git repository.'
                  : 'Builds run through pipelines under projects. Create your first project before triggering builds.'}
              </p>
              <SetupHint
                title="Fastest path to the first build"
                items={[
                  runtimeMode === 'local'
                    ? 'Create a project from a local repository path on the runner host.'
                    : 'Connect GitHub or GitLab if you want repository discovery and webhook triggers.',
                  'Create a pipeline and pick the platforms Oore should build.',
                  'Trigger the first build from the pipeline page, or let a configured webhook do it.',
                ]}
              />

              <div className="flex flex-wrap items-center gap-2">
                {canWriteProjects ? (
                  <Button render={<Link to="/projects" />} nativeButton={false}>
                    <HugeiconsIcon icon={Add01Icon} size={16} />
                    Go To Projects
                  </Button>
                ) : (
                  <p className="text-xs text-muted-foreground">
                    Ask an owner/admin/developer to create the first project.
                  </p>
                )}

                {runtimeMode === 'remote' ? (
                  canWriteIntegrations ? (
                    <Button
                      variant="outline"
                      render={<Link to={integrationConnectTo} />}
                      nativeButton={false}
                    >
                      Connect Source
                    </Button>
                  ) : (
                    <p className="text-xs text-muted-foreground">
                      Ask an owner/admin to connect a source.
                    </p>
                  )
                ) : null}
              </div>
            </CardContent>
          </Card>
        ) : (
          <Card>
            <CardHeader>
              <div className="flex items-center justify-between">
                <CardTitle className="text-sm font-medium uppercase tracking-wider text-muted-foreground">
                  Build queue and history
                </CardTitle>
                <span className="text-xs text-muted-foreground">
                  {total} total
                </span>
              </div>
            </CardHeader>
            <CardContent>
              {builds.length === 0 ? (
                <div className="space-y-2 py-6">
                  <p className="text-sm text-muted-foreground">
                    No builds yet.
                  </p>
                  {canTriggerBuild ? (
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
                      <TableHead>Trigger</TableHead>
                      <TableHead>Branch</TableHead>
                      <TableHead>Commit</TableHead>
                      <TableHead>Created</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {builds.map((build) => (
                      <TableRow
                        key={build.id}
                        className="group cursor-pointer"
                        role="link"
                        tabIndex={0}
                        onClick={() =>
                          void navigate({
                            to: '/builds/$buildId',
                            params: { buildId: build.id },
                          })
                        }
                        onKeyDown={(e) => {
                          if (e.key === 'Enter' || e.key === ' ') {
                            e.preventDefault()
                            void navigate({
                              to: '/builds/$buildId',
                              params: { buildId: build.id },
                            })
                          }
                        }}
                      >
                        <TableCell>
                          <div>
                            <p className="font-mono text-sm group-hover:underline">
                              #{build.build_number}
                            </p>
                            <p className="font-mono text-[11px] text-muted-foreground">
                              {build.id.slice(0, 8)}
                            </p>
                          </div>
                        </TableCell>
                        <TableCell>
                          <Badge variant={getStatusVariant(build.status)}>
                            {build.status}
                          </Badge>
                        </TableCell>
                        <TableCell>
                          <div className="flex items-center gap-2">
                            <Badge variant="outline">
                              {build.trigger_type}
                            </Badge>
                            {build.trigger_actor ? (
                              <span className="text-xs text-muted-foreground">
                                by {build.trigger_actor}
                              </span>
                            ) : null}
                          </div>
                        </TableCell>
                        <TableCell className="font-mono text-xs text-muted-foreground">
                          {build.branch ?? 'n/a'}
                        </TableCell>
                        <TableCell className="font-mono text-xs text-muted-foreground">
                          {build.commit_sha
                            ? build.commit_sha.slice(0, 10)
                            : 'n/a'}
                        </TableCell>
                        <TableCell className="text-sm text-muted-foreground">
                          {relativeTime(build.created_at)}
                        </TableCell>
                      </TableRow>
                    ))}
                  </TableBody>
                </Table>
              )}
            </CardContent>
          </Card>
        )
      ) : null}

      {!isLoading && !error && totalPages > 1 ? (
        <div className="flex items-center justify-between">
          <span className="text-xs text-muted-foreground">
            Page {page} of {totalPages}
          </span>
          <Pagination>
            <PaginationContent>
              <PaginationItem>
                <PaginationPrevious
                  onClick={(e) => {
                    e.preventDefault()
                    if (page > 1)
                      void navigate({
                        search: { page: page - 1 > 1 ? page - 1 : undefined },
                      })
                  }}
                  aria-disabled={page <= 1}
                  className={page <= 1 ? 'pointer-events-none opacity-50' : ''}
                />
              </PaginationItem>
              {Array.from({ length: Math.min(totalPages, 5) }, (_, i) => {
                let pageNum: number
                if (totalPages <= 5) {
                  pageNum = i + 1
                } else if (page <= 3) {
                  pageNum = i + 1
                } else if (page >= totalPages - 2) {
                  pageNum = totalPages - 4 + i
                } else {
                  pageNum = page - 2 + i
                }
                return (
                  <PaginationItem key={pageNum}>
                    <PaginationLink
                      isActive={pageNum === page}
                      onClick={(e) => {
                        e.preventDefault()
                        void navigate({
                          search: {
                            page: pageNum > 1 ? pageNum : undefined,
                          },
                        })
                      }}
                    >
                      {pageNum}
                    </PaginationLink>
                  </PaginationItem>
                )
              })}
              <PaginationItem>
                <PaginationNext
                  onClick={(e) => {
                    e.preventDefault()
                    if (page < totalPages)
                      void navigate({ search: { page: page + 1 } })
                  }}
                  aria-disabled={page >= totalPages}
                  className={
                    page >= totalPages ? 'pointer-events-none opacity-50' : ''
                  }
                />
              </PaginationItem>
            </PaginationContent>
          </Pagination>
        </div>
      ) : null}

      <TriggerBuildDialog
        open={triggerBuildOpen}
        onOpenChange={setTriggerBuildOpen}
        description="Choose a project and pipeline to run a manual build."
        onBuildCreated={(buildId) => {
          void navigate({
            to: '/builds/$buildId',
            params: { buildId },
          })
        }}
      />
    </PageLayout>
  )
}
