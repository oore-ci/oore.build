import { createFileRoute, useNavigate } from '@tanstack/react-router'
import { useEffect, useMemo, useState } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import { InformationCircleIcon, PlayIcon } from '@hugeicons/core-free-icons'

import {
  getActiveInstanceOrRedirect,
  requireAuthOrRedirect,
} from '@/lib/instance-context'
import { useBuilds } from '@/hooks/use-builds'
import { useHasPermission } from '@/hooks/use-permissions'
import { getStatusVariant } from '@/lib/status-variants'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
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
import TriggerBuildDialog from '@/components/trigger-build-dialog'

export const Route = createFileRoute('/builds/')({
  staticData: { breadcrumbLabel: 'Builds' },
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireAuthOrRedirect(instance.id)
  },
  component: BuildsListPage,
})

function BuildsListPage() {
  const navigate = useNavigate()
  const { data, isLoading, error } = useBuilds({ limit: 100 })
  const canTriggerBuild = useHasPermission('builds', 'write')
  const [triggerBuildOpen, setTriggerBuildOpen] = useState(false)

  useEffect(() => {
    document.title = webPageTitle('Builds')
  }, [])

  const builds = useMemo(() => data?.builds ?? [], [data?.builds])

  return (
    <PageLayout width="wide">
      <PageHeader
        title="Builds"
        description="Queue, execution, and historical run inventory across projects."
        actions={
          canTriggerBuild ? (
            <Button onClick={() => setTriggerBuildOpen(true)}>
              <HugeiconsIcon icon={PlayIcon} size={16} />
              Run Build
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
            Failed to load builds: {error.message}
          </AlertDescription>
        </Alert>
      ) : null}

      {!isLoading && !error ? (
        <Card>
          <CardHeader>
            <div className="flex items-center justify-between">
              <CardTitle className="text-sm font-medium uppercase tracking-wider text-muted-foreground">
                Build queue and history
              </CardTitle>
              <span className="text-xs text-muted-foreground">
                {builds.length} total
              </span>
            </div>
          </CardHeader>
          <CardContent>
            {builds.length === 0 ? (
              <div className="space-y-2 py-6">
                <p className="text-sm text-muted-foreground">No builds yet.</p>
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
                      onClick={() =>
                        void navigate({
                          to: '/builds/$buildId',
                          params: { buildId: build.id },
                        })
                      }
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
                          <Badge variant="outline">{build.trigger_type}</Badge>
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
                        {new Date(build.created_at * 1000).toLocaleString()}
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            )}
          </CardContent>
        </Card>
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
