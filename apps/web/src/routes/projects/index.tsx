import { useEffect, useMemo, useState } from 'react'
import { createFileRoute, useNavigate } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import { Add01Icon, InformationCircleIcon } from '@hugeicons/core-free-icons'

import CreateProjectDialog from './-create-project-dialog'
import {
  getActiveInstanceOrRedirect,
  requireAuthOrRedirect,
} from '@/lib/instance-context'
import { useProjects } from '@/hooks/use-projects'
import { useHasPermission } from '@/hooks/use-permissions'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
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
import { webPageTitle } from '@/lib/seo'

export const Route = createFileRoute('/projects/')({
  staticData: { breadcrumbLabel: 'Projects' },
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireAuthOrRedirect(instance.id)
  },
  component: ProjectsListPage,
})

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

function ProjectsListPage() {
  const navigate = useNavigate()
  const { data, isLoading, error } = useProjects({ limit: 100 })
  const canWrite = useHasPermission('projects', 'write')
  const [createOpen, setCreateOpen] = useState(false)

  useEffect(() => {
    document.title = webPageTitle('Projects')
  }, [])

  const projects = useMemo(() => data?.projects ?? [], [data?.projects])

  return (
    <PageLayout width="wide">
      <PageHeader
        title="Projects"
        description="Repository and pipeline entry points for your build system."
        actions={
          canWrite ? (
            <Button onClick={() => setCreateOpen(true)}>
              <HugeiconsIcon icon={Add01Icon} size={16} />
              New Project
            </Button>
          ) : undefined
        }
      />

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
            Failed to load projects: {error.message}
          </AlertDescription>
        </Alert>
      ) : null}

      {!isLoading && !error ? (
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
            {projects.length === 0 ? (
              <div className="space-y-4 py-12 text-center">
                <p className="text-sm text-muted-foreground">
                  No projects yet.
                </p>
                {canWrite ? (
                  <Button onClick={() => setCreateOpen(true)}>
                    <HugeiconsIcon icon={Add01Icon} size={16} />
                    Create your first project
                  </Button>
                ) : null}
              </div>
            ) : (
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
                      onClick={() =>
                        void navigate({
                          to: '/projects/$projectId',
                          params: { projectId: project.id },
                        })
                      }
                    >
                      <TableCell>
                        <div>
                          <p className="font-medium group-hover:underline">
                            {project.name}
                          </p>
                          <p className="font-mono text-[11px] text-muted-foreground">
                            {project.id.slice(0, 8)}
                          </p>
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
            )}
          </CardContent>
        </Card>
      ) : null}

      <CreateProjectDialog open={createOpen} onOpenChange={setCreateOpen} />
    </PageLayout>
  )
}
