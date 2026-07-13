import { useMemo, useState } from 'react'
import {
  Link,
  createFileRoute,
  useNavigate,
  useSearch,
} from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  Add01Icon,
  Folder02Icon,
  InformationCircleIcon,
  Link04Icon,
} from '@hugeicons/core-free-icons'

import CreateProjectDialog from './-create-project-dialog'
import {
  getActiveInstanceOrRedirect,
  requireAuthOrRedirect,
} from '@/lib/instance-context'
import { useIntegrations } from '@/hooks/use-integrations'
import { useProjects } from '@/hooks/use-projects'
import { useHasPermission } from '@/hooks/use-permissions'
import { useSetupStatus } from '@/hooks/use-setup'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import PageHeader from '@/components/page-header'
import PageLayout from '@/components/page-layout'
import { Skeleton } from '@/components/ui/skeleton'
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from '@/components/ui/empty'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import { relativeTime } from '@/lib/format-utils'
import { PageMeta } from '@/lib/seo'
import RepositoryAvatar from '@/components/repository-avatar'
import { READ_ONLY_REASON, isDemoMode } from '@/lib/demo-mode'

export const Route = createFileRoute('/projects/')({
  staticData: { breadcrumbLabel: 'Projects' },
  validateSearch: (
    search: Record<string, unknown>,
  ): { openCreate?: string } => ({
    openCreate: (search.openCreate as string) || undefined,
  }),
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireAuthOrRedirect(instance.id)
  },
  component: ProjectsListPage,
})

function ProjectsListPage() {
  const search = useSearch({ from: '/projects/' })
  const navigate = useNavigate()
  const { data, isLoading, error } = useProjects({ limit: 100 })
  const integrationsQuery = useIntegrations()
  const setupStatusQuery = useSetupStatus()
  const canWriteProjects = useHasPermission('projects', 'write')
  const canWriteIntegrations = useHasPermission('integrations', 'write')
  const [createOpen, setCreateOpen] = useState(false)

  const projects = useMemo(() => data?.projects ?? [], [data?.projects])
  const integrations = useMemo(
    () => integrationsQuery.data?.integrations ?? [],
    [integrationsQuery.data?.integrations],
  )
  const activeIntegrationsCount = useMemo(
    () =>
      integrations.filter((integration) => integration.status === 'active')
        .length,
    [integrations],
  )
  const runtimeMode = setupStatusQuery.data?.runtime_mode ?? 'local'
  const integrationConnectTo = '/settings/integrations'
  const integrationsResolved =
    !integrationsQuery.isLoading && !integrationsQuery.error
  const noConnectedSources =
    runtimeMode === 'remote' &&
    integrationsResolved &&
    activeIntegrationsCount === 0
  const projectsLoading = isLoading || integrationsQuery.isLoading
  const projectsError = error ?? integrationsQuery.error

  const openCreateFromSearch =
    search.openCreate === '1' &&
    !projectsLoading &&
    !projectsError &&
    canWriteProjects
  const isCreateOpen = createOpen || openCreateFromSearch

  function handleCreateOpenChange(open: boolean) {
    setCreateOpen(open)
    if (!open && search.openCreate === '1') {
      void navigate({ to: '/projects', search: {}, replace: true })
    }
  }

  return (
    <PageLayout width="wide">
      <PageMeta title="Projects" noindex />
      <PageHeader
        title="Projects"
        description="Repository and pipeline entry points for your build system."
        actions={
          projects.length > 0 && canWriteProjects ? (
            <Button
              onClick={() => setCreateOpen(true)}
              disabled={isDemoMode}
              title={isDemoMode ? READ_ONLY_REASON : undefined}
            >
              <HugeiconsIcon icon={Add01Icon} />
              New project
            </Button>
          ) : undefined
        }
      />

      {projectsLoading ? (
        <Card>
          <CardContent className="space-y-3">
            <Skeleton className="h-10 w-full" />
            <Skeleton className="h-10 w-full" />
            <Skeleton className="h-10 w-full" />
          </CardContent>
        </Card>
      ) : null}

      {projectsError ? (
        <Alert variant="destructive">
          <HugeiconsIcon icon={InformationCircleIcon} size={16} />
          <AlertDescription>
            Failed to load projects: {projectsError.message}
          </AlertDescription>
        </Alert>
      ) : null}

      {!projectsLoading && !projectsError && projects.length === 0 ? (
        <Empty className="border bg-card">
          <EmptyHeader>
            <EmptyMedia variant="icon">
              <HugeiconsIcon icon={Folder02Icon} />
            </EmptyMedia>
            <EmptyTitle>Create your first project</EmptyTitle>
            <EmptyDescription>
              {runtimeMode === 'local'
                ? 'Choose a local Git repository to create your first project.'
                : noConnectedSources
                  ? 'Connect a source before creating your first remote project.'
                  : 'Create a project from a connected source repository to define pipelines and start builds.'}
            </EmptyDescription>
          </EmptyHeader>
          <EmptyContent>
            <div className="flex flex-col gap-2 sm:flex-row sm:items-center">
              {runtimeMode === 'remote' && noConnectedSources ? (
                canWriteIntegrations ? (
                  <Button
                    render={<Link to={integrationConnectTo} />}
                    nativeButton={false}
                  >
                    <HugeiconsIcon icon={Link04Icon} />
                    Connect source
                  </Button>
                ) : (
                  <p className="text-xs text-muted-foreground">
                    Ask an owner/admin to connect a source.
                  </p>
                )
              ) : canWriteProjects ? (
                <Button
                  onClick={() => setCreateOpen(true)}
                  disabled={isDemoMode}
                  title={isDemoMode ? READ_ONLY_REASON : undefined}
                >
                  <HugeiconsIcon icon={Add01Icon} />
                  Create project
                </Button>
              ) : (
                <p className="text-xs text-muted-foreground">
                  Ask an owner/admin/developer to create the first project.
                </p>
              )}
            </div>
          </EmptyContent>
        </Empty>
      ) : null}

      {!projectsLoading && !projectsError && projects.length > 0 ? (
        <Card>
          <CardHeader>
            <div className="flex items-center justify-between">
              <CardTitle className="text-sm font-medium uppercase tracking-wider text-muted-foreground">
                Project inventory
              </CardTitle>
              <span className="text-xs text-muted-foreground">
                {projects.length} total
              </span>
            </div>
          </CardHeader>
          <CardContent>
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Project</TableHead>
                  <TableHead>Default branch</TableHead>
                  <TableHead>Description</TableHead>
                  <TableHead>Updated</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {projects.map((project) => (
                  <TableRow
                    key={project.id}
                    className="group cursor-pointer"
                    role="link"
                    tabIndex={0}
                    onClick={() =>
                      void navigate({
                        to: '/projects/$projectId',
                        params: { projectId: project.id },
                      })
                    }
                    onKeyDown={(e) => {
                      if (e.key === 'Enter' || e.key === ' ') {
                        e.preventDefault()
                        void navigate({
                          to: '/projects/$projectId',
                          params: { projectId: project.id },
                        })
                      }
                    }}
                  >
                    <TableCell>
                      <div className="flex items-center gap-3">
                        {project.repository_full_name ? (
                          <RepositoryAvatar
                            fullName={project.repository_full_name}
                            avatarUrl={project.repository_avatar_url}
                          />
                        ) : null}
                        <div>
                          <p className="font-medium group-hover:underline">
                            {project.name}
                          </p>
                          <p className="font-mono text-[11px] text-muted-foreground">
                            {project.repository_full_name ??
                              project.id.slice(0, 8)}
                          </p>
                        </div>
                      </div>
                    </TableCell>
                    <TableCell className="font-mono text-xs text-muted-foreground">
                      {project.default_branch ?? 'not set'}
                    </TableCell>
                    <TableCell className="max-w-[30ch] truncate text-sm text-muted-foreground">
                      {project.description ?? 'No description'}
                    </TableCell>
                    <TableCell className="text-sm text-muted-foreground">
                      {relativeTime(project.updated_at)}
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </CardContent>
        </Card>
      ) : null}

      <CreateProjectDialog
        open={isCreateOpen}
        onOpenChange={handleCreateOpenChange}
      />
    </PageLayout>
  )
}
