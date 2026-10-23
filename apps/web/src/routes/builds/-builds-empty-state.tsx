import { Link } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  ArrowRight01Icon,
  Folder02Icon,
  PlayIcon,
} from '@hugeicons/core-free-icons'

import { Button } from '@/components/ui/button'
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from '@/components/ui/empty'
import { Text } from '@/components/layout/text'

export function BuildsEmptyState({
  capabilities,
  state,
  onRunBuild,
  runtimeMode,
}: {
  capabilities: {
    triggerBuild: boolean
    writeProjects: boolean
  }
  state: 'missing-projects' | 'no-builds'
  onRunBuild: () => void
  runtimeMode: 'local' | 'remote'
}) {
  if (state === 'missing-projects') {
    return (
      <Empty className="flex-none border bg-card">
        <EmptyHeader>
          <EmptyMedia variant="icon">
            <HugeiconsIcon icon={Folder02Icon} />
          </EmptyMedia>
          <EmptyTitle>No projects to build yet</EmptyTitle>
          <EmptyDescription>
            {runtimeMode === 'local'
              ? 'Builds run from project pipelines. Create a project from a Git repository on this Mac, add a pipeline, and your builds will show up here.'
              : 'Builds run from project pipelines. Create a project from a GitHub or GitLab repository, add a pipeline, and your builds will show up here.'}
          </EmptyDescription>
        </EmptyHeader>
        <EmptyContent>
          {capabilities.writeProjects ? (
            <Button render={<Link to="/projects" />} nativeButton={false}>
              Set up a project
              <HugeiconsIcon icon={ArrowRight01Icon} data-icon="inline-end" />
            </Button>
          ) : (
            <Text variant="muted">
              Ask an owner or admin to create the first project.
            </Text>
          )}
        </EmptyContent>
      </Empty>
    )
  }

  return (
    <Empty className="border bg-card">
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
          <Button onClick={onRunBuild}>
            <HugeiconsIcon icon={PlayIcon} />
            Run first build
          </Button>
        </EmptyContent>
      ) : null}
    </Empty>
  )
}
