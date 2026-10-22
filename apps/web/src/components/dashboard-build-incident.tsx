import { Link } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  ChevronRightIcon,
  AlertCircleIcon as CircleAlertIcon,
  Alert02Icon as TriangleAlertIcon,
} from '@hugeicons/core-free-icons'

import { Badge } from '@/components/ui/badge'
import {
  Item,
  ItemActions,
  ItemContent,
  ItemDescription,
  ItemGroup,
  ItemMedia,
  ItemTitle,
} from '@/components/ui/item'
import { relativeTime } from '@/lib/format-utils'
import { getRunnerPolicyBlockLabel } from '@/lib/status-variants'
import type { Build, RunnerPolicyBlockReason } from '@oore/client/models'

const isFailure = (build: Build) =>
  build.status === 'failed' || build.status === 'timed_out'

/** A project whose newest verdict, in at least one pipeline, is a failure. */
export type FailingProject = {
  projectId: string
  projectName: string
  pipelineNames: Array<string>
  failures: number
  latest: Build
}

/**
 * Projects that are failing right now. A pipeline counts while its newest
 * passed-or-failed build failed; one success clears it. Canceled and expired
 * builds give no verdict and are skipped.
 */
export function failingProjects(finished: Array<Build>): Array<FailingProject> {
  const newestFirst = [...finished].sort(
    (a, b) => (b.finished_at ?? b.queued_at) - (a.finished_at ?? a.queued_at),
  )
  const settled = new Set<string>()
  const projects = new Map<string, FailingProject>()
  for (const build of newestFirst) {
    if (settled.has(build.pipeline_id)) continue
    if (build.status === 'succeeded') {
      settled.add(build.pipeline_id)
      continue
    }
    if (!isFailure(build)) continue
    const project = projects.get(build.project_id)
    const pipelineName = build.context?.pipeline_name ?? 'Pipeline'
    if (!project) {
      projects.set(build.project_id, {
        projectId: build.project_id,
        projectName: build.context?.project_name ?? build.project_id,
        pipelineNames: [pipelineName],
        failures: 1,
        latest: build,
      })
      continue
    }
    project.failures += 1
    if (!project.pipelineNames.includes(pipelineName)) {
      project.pipelineNames.push(pipelineName)
    }
  }
  return [...projects.values()]
}

function blockDescription(reason: RunnerPolicyBlockReason): string {
  if (reason === 'repository_unavailable')
    return "Oore can't check out the source"
  if (reason === 'instance_paused') return 'Mac builds are paused'
  return getRunnerPolicyBlockLabel(reason)
}

/** Blocked builds per project, oldest first within each. */
function blockedGroups(blocked: Array<Build>) {
  const groups = new Map<string, Array<Build>>()
  for (const build of [...blocked].sort((a, b) => a.queued_at - b.queued_at)) {
    if (!build.runner_policy_block_reason) continue
    groups.set(build.project_id, [
      ...(groups.get(build.project_id) ?? []),
      build,
    ])
  }
  return [...groups.values()]
}

function AttentionItem({
  title,
  description,
  link,
  icon,
}: {
  title: string
  description: string
  link: React.ReactElement
  icon: typeof TriangleAlertIcon
}) {
  return (
    <Item variant="outline" size="sm" render={link}>
      <ItemMedia variant="icon">
        <HugeiconsIcon icon={icon} className="text-warning!" />
      </ItemMedia>
      <ItemContent className="min-w-0">
        <ItemTitle>{title}</ItemTitle>
        <ItemDescription className="line-clamp-1">
          {description}
        </ItemDescription>
      </ItemContent>
      <ItemActions>
        <HugeiconsIcon icon={ChevronRightIcon} />
      </ItemActions>
    </Item>
  )
}

function failingDescription(project: FailingProject): string {
  const where =
    project.pipelineNames.length === 1
      ? project.pipelineNames[0]
      : `${project.pipelineNames.length} pipelines`
  const count =
    project.failures === 1
      ? 'Last build failed'
      : `${project.failures} failures since the last pass`
  const latest = project.latest
  return `${where} · ${count} · #${latest.build_number} ${relativeTime(latest.finished_at ?? latest.queued_at)}`
}

/** What is broken right now, one line per project or cause, gone once it recovers. */
export default function DashboardBuildIncident({
  blocked,
  failing,
  noOnlineRunners,
}: {
  blocked: Array<Build>
  failing: Array<FailingProject>
  noOnlineRunners: boolean
}) {
  const blockedByProject = blockedGroups(blocked)
  const count =
    failing.length + blockedByProject.length + (noOnlineRunners ? 1 : 0)
  if (count === 0) return null

  return (
    <section className="flex flex-col gap-3" aria-labelledby="attention-needed">
      <div className="flex items-center gap-2">
        <h2
          id="attention-needed"
          className="text-sm font-medium text-muted-foreground"
        >
          Needs attention
        </h2>
        <Badge variant="outline">{count}</Badge>
      </div>

      <ItemGroup className="gap-2">
        {noOnlineRunners ? (
          <AttentionItem
            icon={CircleAlertIcon}
            title="No runner is online"
            description="Builds wait until a runner checks in."
            link={
              <Link
                to="/settings/runners"
                aria-label="Review runner availability"
              />
            }
          />
        ) : null}

        {blockedByProject.map((builds) => {
          const oldest = builds[0]
          const projectName = oldest.context?.project_name ?? oldest.project_id
          const reason = oldest.runner_policy_block_reason
          const waiting =
            builds.length === 1 ? '1 build' : `${builds.length} builds`
          return (
            <AttentionItem
              key={oldest.project_id}
              icon={CircleAlertIcon}
              title={`${projectName} builds are blocked`}
              description={`${reason ? blockDescription(reason) : 'Waiting'} · ${waiting} waiting since ${relativeTime(oldest.queued_at)}`}
              link={
                <Link
                  to="/builds/$buildId"
                  params={{ buildId: oldest.id }}
                  aria-label={`Open ${projectName} build #${oldest.build_number}, blocked the longest`}
                />
              }
            />
          )
        })}

        {failing.map((project) => (
          <AttentionItem
            key={project.projectId}
            icon={TriangleAlertIcon}
            title={`${project.projectName} builds are failing`}
            description={failingDescription(project)}
            link={
              <Link
                to="/builds/$buildId"
                params={{ buildId: project.latest.id }}
                aria-label={`Open ${project.projectName} build #${project.latest.build_number}, the latest failure`}
              />
            }
          />
        ))}
      </ItemGroup>
    </section>
  )
}
