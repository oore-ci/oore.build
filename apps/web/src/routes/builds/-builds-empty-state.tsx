import { Link } from '@tanstack/react-router'
import {
  ArrowRight01Icon,
  Link04Icon,
  PlayIcon,
  Search01Icon,
} from '@hugeicons/core-free-icons'
import { HugeiconsIcon } from '@hugeicons/react'

import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from '@/components/ui/empty'
import SetupHint from '@/components/setup-hint'

export function BuildsEmptyState({
  capabilities,
  state,
  onClearFilters,
  onRunBuild,
  onWarmBuildDialog,
  runtimeMode,
}: {
  capabilities: {
    triggerBuild: boolean
    writeIntegrations: boolean
    writeProjects: boolean
  }
  state: 'missing-projects' | 'no-builds' | 'no-results' | null
  onClearFilters: () => void
  onRunBuild: () => void
  onWarmBuildDialog: () => void
  runtimeMode: 'local' | 'remote'
}) {
  if (state === 'missing-projects') {
    return (
      <Card>
        <CardHeader>
          <CardTitle className="text-sm font-medium uppercase tracking-wider text-muted-foreground">
            Create project first
          </CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          <p className="text-sm text-muted-foreground">
            {runtimeMode === 'local'
              ? 'Builds run through project pipelines. Create your first project from a local Git repository.'
              : 'Builds run through project pipelines. Create your first project before triggering builds.'}
          </p>
          <SetupHint
            title="Fastest path to the first build"
            items={[
              runtimeMode === 'local'
                ? 'Create a project from a local repository path on the runner host.'
                : 'Connect GitHub or GitLab for repository discovery and webhook triggers.',
              'Create a pipeline and choose the platforms Oore should build.',
              'Trigger the first build from the pipeline page or a configured webhook.',
            ]}
          />
          <div className="flex flex-wrap items-center gap-2">
            {capabilities.writeProjects ? (
              <Button render={<Link to="/projects" />} nativeButton={false}>
                Go to projects
                <HugeiconsIcon icon={ArrowRight01Icon} />
              </Button>
            ) : (
              <p className="text-xs text-muted-foreground">
                Ask an owner, admin, or developer to create the first project.
              </p>
            )}
            {runtimeMode === 'remote' && capabilities.writeIntegrations ? (
              <Button
                variant="outline"
                render={<Link to="/settings/integrations" />}
                nativeButton={false}
              >
                <HugeiconsIcon icon={Link04Icon} />
                Connect source
              </Button>
            ) : null}
          </div>
        </CardContent>
      </Card>
    )
  }

  if (state === 'no-builds') {
    return (
      <Empty className="bg-card">
        <EmptyHeader>
          <EmptyMedia variant="icon">
            <HugeiconsIcon icon={PlayIcon} />
          </EmptyMedia>
          <EmptyTitle>No builds yet</EmptyTitle>
          <EmptyDescription>
            Run a pipeline to see its status, output, and artifacts here.
          </EmptyDescription>
        </EmptyHeader>
        {capabilities.triggerBuild ? (
          <EmptyContent>
            <Button
              onMouseEnter={onWarmBuildDialog}
              onFocus={onWarmBuildDialog}
              onClick={onRunBuild}
            >
              <HugeiconsIcon icon={PlayIcon} />
              Run first build
            </Button>
          </EmptyContent>
        ) : null}
      </Empty>
    )
  }

  return state === 'no-results' ? (
    <Empty className="bg-card">
      <EmptyHeader>
        <EmptyMedia variant="icon">
          <HugeiconsIcon icon={Search01Icon} />
        </EmptyMedia>
        <EmptyTitle>No matching builds</EmptyTitle>
        <EmptyDescription>
          Change the current filters or clear them to see all builds.
        </EmptyDescription>
      </EmptyHeader>
      <EmptyContent>
        <Button variant="outline" onClick={onClearFilters}>
          Clear filters
        </Button>
      </EmptyContent>
    </Empty>
  ) : null
}
