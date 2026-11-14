import { useState } from 'react'
import { Link, useNavigate } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import { CheckmarkCircle02Icon } from '@hugeicons/core-free-icons'
import type { Project } from '@oore/client/models'
import { toast } from '@/lib/toast'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import { Spinner } from '@/components/ui/spinner'
import { Row } from '@/components/layout/row'
import { Stack } from '@/components/layout/stack'
import { Text } from '@/components/layout/text'
import { ToolchainChecks } from '@/components/runner-toolchain'
import { useCreateBuild } from '@/hooks/use-builds'
import { useCreatePipeline } from '@/hooks/use-pipelines'
import { useProjectAppDetection } from '@/hooks/use-projects'
import { useRunners } from '@/hooks/use-runners'
import { proposedPipelines, type ProposedPipeline } from '@/lib/first-pipelines'
import {
  PLATFORM_LABEL,
  platformReadiness,
  type PlatformReadiness,
} from '@/lib/runner-toolchain'

function listWords(words: Array<string>) {
  if (words.length <= 1) return words.join('')
  return `${words.slice(0, -1).join(', ')} and ${words.at(-1)}`
}

function ReadinessLine({ readiness }: { readiness: PlatformReadiness }) {
  switch (readiness.state) {
    case 'ready':
      return (
        <Stack gap={1}>
          <Row gap={1} className="text-xs text-muted-foreground">
            <HugeiconsIcon
              icon={CheckmarkCircle02Icon}
              className="size-3.5 text-success"
              aria-hidden
            />
            Builds on {readiness.runner.name}
          </Row>
          <ToolchainChecks checks={readiness.warnings} />
        </Stack>
      )
    case 'missing':
      return (
        <Stack gap={1}>
          <Text variant="caption">
            {readiness.runner.name} cannot build this yet. Fix these on the Mac,
            and the runner notices within 15 minutes.
          </Text>
          <ToolchainChecks checks={readiness.problems} />
        </Stack>
      )
    case 'offline':
      return (
        <Text variant="caption">
          No runner is online, so the build waits in the queue.{' '}
          <Link to="/settings/runners" className="underline">
            Check runners
          </Link>
        </Text>
      )
    case 'unknown':
      return null
  }
}

/**
 * The "Add pipeline" step of the first-app checklist, done for the person:
 * Oore reads the repository, proposes a pipeline per platform, says whether
 * a runner can build each one, and starts the first build.
 */
export function FirstPipelinesSetup({ project }: { project: Project }) {
  const navigate = useNavigate()
  const detection = useProjectAppDetection(project.id)
  // Developers may not list runners; the proposal works without them.
  const runnersQuery = useRunners(undefined, { enabled: true })
  const runners = runnersQuery.data?.runners ?? []
  const createPipeline = useCreatePipeline()
  const createBuild = useCreateBuild()
  const [deselected, setDeselected] = useState<Record<string, boolean>>({})
  const [pending, setPending] = useState(false)

  const manual = (
    <Button
      size="sm"
      variant="ghost"
      render={
        <Link
          to="/projects/$projectId/pipelines/new"
          params={{ projectId: project.id }}
        />
      }
      nativeButton={false}
    >
      Set up manually
    </Button>
  )

  if (detection.isPending) {
    return (
      <Row gap={2} className="text-sm text-muted-foreground">
        <Spinner className="size-4" />
        Reading {project.repository_full_name ?? 'the repository'}...
      </Row>
    )
  }
  if (detection.error || !detection.data) {
    return (
      <Stack gap={2}>
        <Text variant="muted">
          Oore could not read the repository
          {detection.error ? `: ${detection.error.message}` : ''}. Add the
          pipeline yourself instead.
        </Text>
        <Row gap={2}>
          <Button
            size="sm"
            render={
              <Link
                to="/projects/$projectId/pipelines/new"
                params={{ projectId: project.id }}
              />
            }
            nativeButton={false}
          >
            Add pipeline
          </Button>
        </Row>
      </Stack>
    )
  }

  const found = detection.data
  const proposals = proposedPipelines(found, {
    manualOnlyTriggers: project.repository_provider === 'local_git',
  })
  const isSelected = (proposal: ProposedPipeline) =>
    deselected[proposal.key] ?? proposal.selected
  const selected = proposals.filter(isSelected)
  const readiness = (proposal: ProposedPipeline) =>
    proposal.platform && runnersQuery.data
      ? platformReadiness(runners, proposal.platform)
      : undefined
  const startsBuild = selected.some(
    (proposal) => proposal.runsNow && readiness(proposal)?.state !== 'missing',
  )

  const summary =
    found.config_files.length > 0
      ? `Found ${listWords(found.config_files)} on ${found.reference}.`
      : found.flutter
        ? `Found a Flutter app${found.app_name ? `, ${found.app_name},` : ''} with ${
            found.platforms.length
              ? listWords(found.platforms.map((p) => PLATFORM_LABEL[p]))
              : 'no platform folders'
          } on ${found.reference}.`
        : `No Flutter app on ${found.reference}: there is no pubspec.yaml that uses Flutter.`

  async function handleCreate() {
    setPending(true)
    let firstBuildId: string | undefined
    let iosPipelineId: string | undefined
    try {
      for (const proposal of selected) {
        const { pipeline } = await createPipeline.mutateAsync({
          projectId: project.id,
          data: proposal.request,
        })
        if (proposal.platform === 'ios') iosPipelineId = pipeline.id
        if (
          proposal.runsNow &&
          readiness(proposal)?.state !== 'missing' &&
          !firstBuildId
        ) {
          const { build } = await createBuild.mutateAsync({
            projectId: project.id,
            data: { pipeline_id: pipeline.id, branch: found.reference },
          })
          firstBuildId = build.id
        }
      }
    } catch (error) {
      const message = error instanceof Error ? error.message : 'Unknown error'
      toast.error(`Could not finish setting up: ${message}`)
      setPending(false)
      return
    }
    setPending(false)
    if (firstBuildId) {
      toast.success('First build started')
      void navigate({
        to: '/builds/$buildId',
        params: { buildId: firstBuildId },
      })
    } else if (iosPipelineId) {
      toast.success('Pipeline created. Add iOS signing to build it.')
      void navigate({
        to: '/projects/$projectId/pipelines/$pipelineId/edit',
        params: { projectId: project.id, pipelineId: iosPipelineId },
        hash: 'ios-signing',
      })
    } else {
      toast.success('Pipeline created')
    }
  }

  return (
    <Stack gap={3} className="pt-1">
      <Text variant="muted">{summary}</Text>
      {proposals.length === 0 ? (
        <Row gap={2}>{manual}</Row>
      ) : (
        <>
          <ul className="flex flex-col gap-2" aria-label="Proposed pipelines">
            {proposals.map((proposal) => {
              const ready = readiness(proposal)
              return (
                <li
                  key={proposal.key}
                  className="flex items-start gap-3 rounded-lg border p-3"
                >
                  <Checkbox
                    id={`first-pipeline-${proposal.key}`}
                    checked={isSelected(proposal)}
                    onCheckedChange={(checked) =>
                      setDeselected((previous) => ({
                        ...previous,
                        [proposal.key]: checked === true,
                      }))
                    }
                    className="mt-0.5"
                  />
                  <Stack gap={1} className="min-w-0 flex-1">
                    <label
                      htmlFor={`first-pipeline-${proposal.key}`}
                      className="text-sm font-medium"
                    >
                      {proposal.title}
                    </label>
                    <Text variant="caption">{proposal.detail}</Text>
                    {ready && isSelected(proposal) ? (
                      <ReadinessLine readiness={ready} />
                    ) : null}
                  </Stack>
                </li>
              )
            })}
          </ul>
          <Row gap={2} className="flex-wrap">
            <Button
              size="sm"
              disabled={selected.length === 0 || pending}
              onClick={() => void handleCreate()}
            >
              {pending ? <Spinner className="size-4" /> : null}
              {startsBuild
                ? 'Create and start the first build'
                : selected.length > 1
                  ? 'Create pipelines'
                  : 'Create pipeline'}
            </Button>
            {manual}
          </Row>
        </>
      )}
    </Stack>
  )
}
