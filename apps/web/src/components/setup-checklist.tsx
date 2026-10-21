import { useEffect } from 'react'
import { Link } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  ArrowRight01Icon,
  CheckmarkCircle02Icon,
  Download04Icon,
} from '@hugeicons/core-free-icons'
import { useProject } from '@/hooks/use-projects'
import { useBuilds, useProjectArtifacts } from '@/hooks/use-builds'
import {
  hasProjectPermission,
  useHasPermissions,
} from '@/hooks/use-permissions'
import { artifactInstallReadiness } from '@/lib/artifact-install'
import {
  isSetupComplete,
  setupDoneCount,
  setupNext,
  setupSteps,
  type SetupInput,
  type SetupNext,
} from '@/lib/setup-checklist'
import { cn } from '@/lib/utils'
import { useFirstAppScope, useFirstAppStore } from '@/stores/first-app-store'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/components/ui/card'
import {
  Item,
  ItemActions,
  ItemContent,
  ItemDescription,
  ItemTitle,
} from '@/components/ui/item'
import { Skeleton } from '@/components/ui/skeleton'

/**
 * The first-app checklist has one home: the top of the project overview.
 * Home shows the `summary` variant, a single row that leads there. Both
 * disappear on their own once the first installable app exists, so there is
 * no hide or resume state to lose.
 */
export default function SetupChecklist({
  projectId,
  variant = 'full',
}: {
  projectId: string
  variant?: 'full' | 'summary'
}) {
  const scope = useFirstAppScope()
  const update = useFirstAppStore((state) => state.update)
  const [canChangeSource] = useHasPermissions(['instance_settings:write'])
  const projectQuery = useProject(projectId)
  const buildsQuery = useBuilds({ project_id: projectId, limit: 1 })
  const artifactsQuery = useProjectArtifacts(projectId)
  const latestBuild = buildsQuery.data?.builds[0]
  const { refetch: refreshArtifacts } = artifactsQuery
  useEffect(() => {
    if (latestBuild?.status === 'succeeded') void refreshArtifacts()
  }, [latestBuild?.id, latestBuild?.status, refreshArtifacts])

  const artifact = artifactsQuery.data?.artifacts.find(
    (item) =>
      item.state === 'available' && artifactInstallReadiness(item).ready,
  )
  const complete = !!artifact
  useEffect(() => {
    if (complete) update(scope, { projectId: undefined })
  }, [complete, scope, update])

  if (
    projectQuery.isLoading ||
    buildsQuery.isLoading ||
    artifactsQuery.isLoading
  ) {
    return (
      <Skeleton
        className={variant === 'full' ? 'h-64 w-full' : 'h-16 w-full'}
      />
    )
  }
  if (projectQuery.error || buildsQuery.error || artifactsQuery.error) {
    return (
      <Alert>
        <AlertDescription className="flex flex-wrap items-center justify-between gap-3">
          <span>Could not load setup progress.</span>
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
        </AlertDescription>
      </Alert>
    )
  }

  const data = projectQuery.data
  if (!data || complete) return null
  const project = data.project
  const input: SetupInput = {
    hasSource: !!project.repository_id && !!project.repository_full_name,
    hasPipeline: data.pipeline_count > 0,
    latestBuildStatus: latestBuild?.status,
    artifactReady: !!artifact,
  }
  if (isSetupComplete(input)) return null
  const next = setupNext(input)
  const doneCount = setupDoneCount(input)
  const canConfigure = hasProjectPermission(
    data.current_user_role,
    'pipelines:write',
  )
  const canRun = hasProjectPermission(data.current_user_role, 'builds:write')

  if (variant === 'summary') {
    return (
      <Item variant="outline" size="sm">
        <ItemContent>
          <ItemTitle>Set up your first app · {project.name}</ItemTitle>
          <ItemDescription>
            {doneCount} of 5 steps done. Next: {next.title}
          </ItemDescription>
        </ItemContent>
        <ItemActions>
          <Button
            size="sm"
            render={<Link to="/projects/$projectId" params={{ projectId }} />}
            nativeButton={false}
          >
            Continue
            <HugeiconsIcon icon={ArrowRight01Icon} />
          </Button>
        </ItemActions>
      </Item>
    )
  }

  const steps = setupSteps(input)
  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle>Set up your first app</CardTitle>
        <CardDescription>{doneCount} of 5 steps done</CardDescription>
      </CardHeader>
      <CardContent>
        <ol aria-label="Setup steps" className="flex flex-col">
          {steps.map((step, index) => {
            const current = step.id === next.id
            return (
              <li
                key={step.id}
                aria-current={current ? 'step' : undefined}
                className={cn(
                  'flex gap-3 border-b py-3 first:pt-0 last:border-b-0 last:pb-0',
                  !step.done && !current && 'text-muted-foreground',
                )}
              >
                <span
                  aria-hidden
                  className={cn(
                    'mt-0.5 flex size-5 shrink-0 items-center justify-center rounded-full border text-[11px] font-medium tabular-nums',
                    step.done && 'border-success/30 bg-success/10 text-success',
                    current && 'border-primary text-primary',
                  )}
                >
                  {step.done ? (
                    <HugeiconsIcon icon={CheckmarkCircle02Icon} size={14} />
                  ) : (
                    index + 1
                  )}
                </span>
                <div className="flex min-w-0 flex-1 flex-col gap-1">
                  <span className="text-sm font-medium">
                    {step.label}
                    {step.done ? (
                      <span className="sr-only"> (done)</span>
                    ) : null}
                  </span>
                  {current ? (
                    <>
                      <p className="text-sm text-muted-foreground">
                        {next.detail}
                      </p>
                      <SetupAction
                        canChangeSource={canChangeSource}
                        canConfigure={canConfigure}
                        canRun={canRun}
                        latestBuildId={latestBuild?.id}
                        next={next}
                        projectId={projectId}
                      />
                    </>
                  ) : null}
                </div>
              </li>
            )
          })}
        </ol>
      </CardContent>
    </Card>
  )
}

function SetupAction({
  canChangeSource,
  canConfigure,
  canRun,
  latestBuildId,
  next,
  projectId,
}: {
  canChangeSource: boolean
  canConfigure: boolean
  canRun: boolean
  latestBuildId: string | undefined
  next: SetupNext
  projectId: string
}) {
  const hint = (text: string) => (
    <p className="text-sm text-muted-foreground">{text}</p>
  )
  switch (next.id) {
    case 'repository':
      return canChangeSource ? (
        <div className="pt-1">
          <Button
            size="sm"
            render={
              <Link
                to="/projects/$projectId"
                params={{ projectId }}
                search={{ tab: 'settings' }}
              />
            }
            nativeButton={false}
          >
            Connect repository
          </Button>
        </div>
      ) : (
        hint('Ask an owner or admin to connect a repository.')
      )
    case 'pipeline':
      return canConfigure ? (
        <div className="pt-1">
          <Button
            size="sm"
            render={
              <Link
                to="/projects/$projectId/pipelines/new"
                params={{ projectId }}
              />
            }
            nativeButton={false}
          >
            Add pipeline
          </Button>
        </div>
      ) : (
        hint('Ask a project maintainer to add a pipeline.')
      )
    case 'build':
      return canRun ? (
        <div className="pt-1">
          <Button
            size="sm"
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
        </div>
      ) : (
        hint('Ask a project maintainer for permission to run builds.')
      )
    case 'install':
      return latestBuildId ? (
        <div className="pt-1">
          <Button
            size="sm"
            variant="outline"
            render={
              <Link to="/builds/$buildId" params={{ buildId: latestBuildId }} />
            }
            nativeButton={false}
          >
            View build
          </Button>
        </div>
      ) : null
    case 'done':
      return (
        <div className="pt-1">
          <Button
            size="sm"
            render={<Link to="/projects/$projectId" params={{ projectId }} />}
            nativeButton={false}
          >
            <HugeiconsIcon icon={Download04Icon} />
            Install
          </Button>
        </div>
      )
  }
}
