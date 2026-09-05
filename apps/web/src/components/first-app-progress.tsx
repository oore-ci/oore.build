import { Link } from '@tanstack/react-router'
import { useEffect } from 'react'
import { useProject } from '@/hooks/use-projects'
import { useBuilds, useProjectArtifacts } from '@/hooks/use-builds'
import { hasProjectPermission } from '@/hooks/use-permissions'
import { artifactInstallReadiness } from '@/lib/artifact-install'
import { useFirstAppScope, useFirstAppStore } from '@/stores/first-app-store'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Skeleton } from '@/components/ui/skeleton'

export default function FirstAppProgress({ projectId }: { projectId: string }) {
  const scope = useFirstAppScope()
  const hidden = useFirstAppStore((state) => state.progress[scope]?.hidden)
  const update = useFirstAppStore((state) => state.update)
  const projectQuery = useProject(hidden ? '' : projectId)
  const buildsQuery = useBuilds(
    { project_id: projectId, limit: 1 },
    { enabled: !hidden },
  )
  const artifactsQuery = useProjectArtifacts(hidden ? '' : projectId)
  const latestBuild = buildsQuery.data?.builds[0]
  const { refetch: refreshArtifacts } = artifactsQuery
  useEffect(() => {
    if (!hidden && latestBuild?.status === 'succeeded') void refreshArtifacts()
  }, [hidden, latestBuild?.id, latestBuild?.status, refreshArtifacts])

  if (hidden)
    return (
      <Button
        variant="ghost"
        size="sm"
        className="self-start"
        onClick={() => update(scope, { hidden: false })}
      >
        Resume first-app setup
      </Button>
    )
  if (
    projectQuery.isLoading ||
    buildsQuery.isLoading ||
    artifactsQuery.isLoading
  )
    return <Skeleton className="h-40 w-full" />
  if (projectQuery.error || buildsQuery.error || artifactsQuery.error)
    return (
      <Alert>
        <AlertDescription className="flex flex-wrap items-center justify-between gap-3">
          <span>
            Setup progress could not be checked. Your project is unchanged.
          </span>
          <Button
            variant="outline"
            size="sm"
            onClick={() => {
              void projectQuery.refetch()
              void buildsQuery.refetch()
              void artifactsQuery.refetch()
            }}
          >
            Retry
          </Button>
          <Button
            variant="ghost"
            size="sm"
            onClick={() =>
              update(scope, { projectId: undefined, hidden: false })
            }
            render={<Link to="/projects" />}
            nativeButton={false}
          >
            Choose a project
          </Button>
        </AlertDescription>
      </Alert>
    )

  const data = projectQuery.data
  if (!data) return null
  const project = data.project
  const build = buildsQuery.data?.builds[0]
  const artifact = artifactsQuery.data?.artifacts.find(
    (item) =>
      item.state === 'available' && artifactInstallReadiness(item).ready,
  )
  const hasSource = !!project.repository_id && !!project.repository_full_name
  const hasPipeline = data.pipeline_count > 0
  const canConfigure = hasProjectPermission(
    data.current_user_role,
    'pipelines:write',
  )
  const canRun = hasProjectPermission(data.current_user_role, 'builds:write')
  const next = !hasSource
    ? 'Link the repository'
    : !hasPipeline
      ? 'Choose what to build'
      : !build
        ? 'Run your first build'
        : artifact
          ? 'Your app is ready to install'
          : build.status === 'succeeded'
            ? 'Find a usable app output'
            : 'Follow your first build'
  const detail = !hasSource
    ? 'An owner or admin can repair the source in project settings.'
    : !hasPipeline
      ? 'Start with a Flutter Android test app, or use your repository workflow.'
      : !build
        ? 'Review the pipeline and branch, then start the build. The runner checks toolchain and signing prerequisites when it runs.'
        : artifact
          ? 'Open the install page to choose the right device and share the app.'
          : build.status === 'succeeded'
            ? 'The build finished, but no available install-ready app was found in recent outputs. Check artifact paths or iOS signing in the build details.'
            : 'Check progress or the reported problem. You can leave and return here.'
  return (
    <Card size="sm">
      <CardHeader className="flex flex-wrap items-center justify-between gap-3">
        <CardTitle>First app · {project.name}</CardTitle>
        <Button
          variant="ghost"
          size="sm"
          onClick={() => update(scope, { hidden: true })}
        >
          Hide guide
        </Button>
      </CardHeader>
      <CardContent className="space-y-4">
        <ol
          aria-label="First app progress"
          className="flex flex-wrap gap-x-5 gap-y-2 text-sm"
        >
          {[
            ['Repository', hasSource],
            ['Project', true],
            ['Build setup', hasPipeline],
            ['First run', !!build],
            ['Usable app', !!artifact],
          ].map(([label, done], index) => (
            <li key={String(label)} className="flex items-center gap-2">
              <Badge variant={done ? 'secondary' : 'outline'}>
                {done ? 'Done' : index + 1}
              </Badge>
              <span>{label}</span>
            </li>
          ))}
        </ol>
        <div>
          <p className="font-medium">{next}</p>
          <p className="mt-1 text-sm text-muted-foreground">{detail}</p>
        </div>
        <div className="flex flex-wrap gap-2">
          {!hasSource ? (
            <Button
              variant="outline"
              render={
                <Link
                  to="/projects/$projectId"
                  params={{ projectId }}
                  search={{ tab: 'settings' }}
                />
              }
              nativeButton={false}
            >
              Project settings
            </Button>
          ) : !hasPipeline ? (
            canConfigure ? (
              <Button
                render={
                  <Link
                    to="/projects/$projectId/pipelines/new"
                    params={{ projectId }}
                  />
                }
                nativeButton={false}
              >
                Set up a build
              </Button>
            ) : (
              <p className="text-sm">
                Ask a project maintainer to add a pipeline.
              </p>
            )
          ) : !build ? (
            canRun ? (
              <Button
                render={
                  <Link
                    to="/projects/$projectId"
                    params={{ projectId }}
                    search={{ run: '1' }}
                  />
                }
                nativeButton={false}
              >
                Run first build
              </Button>
            ) : (
              <p className="text-sm">
                Ask a project maintainer for permission to run builds.
              </p>
            )
          ) : artifact ? (
            <Button
              render={
                <Link
                  to="/builds/$buildId"
                  params={{ buildId: artifact.build_id }}
                  search={{ install: artifact.id }}
                />
              }
              nativeButton={false}
            >
              Open install page
            </Button>
          ) : (
            <Button
              render={
                <Link to="/builds/$buildId" params={{ buildId: build.id }} />
              }
              nativeButton={false}
            >
              View build
            </Button>
          )}
          <Button
            variant="ghost"
            render={<Link to="/projects/$projectId" params={{ projectId }} />}
            nativeButton={false}
          >
            Open project
          </Button>
        </div>
      </CardContent>
    </Card>
  )
}
