import { Link } from '@tanstack/react-router'
import { DynamicLucideIcon } from '@/components/ui/dynamic-lucide-icon'
import {
  GitBranch as GitBranchIcon,
  GitCommitHorizontal as GitCommitIcon,
  Clock as TimeQuarterPassIcon,
} from 'lucide-react'

import type { Build } from '@/lib/types'
import { formatDuration, relativeTime } from '@/lib/format-utils'

export function BuildSummary({
  build,
  duration,
}: {
  build: Build
  duration: number | null
}) {
  const timingLabel = build.finished_at
    ? `Finished ${relativeTime(build.finished_at)}`
    : build.started_at
      ? `Started ${relativeTime(build.started_at)}`
      : `Queued ${relativeTime(build.queued_at)}`

  return (
    <dl
      aria-label="Build summary"
      className="flex min-w-0 flex-wrap items-center gap-x-4 gap-y-2"
    >
      {build.branch || build.commit_sha ? (
        <div className="flex min-w-0 items-center gap-2">
          <dt className="sr-only">Source</dt>
          <dd className="flex min-w-0 items-center gap-2">
            {build.branch ? (
              <span className="inline-flex min-w-0 items-center gap-1 font-mono text-xs text-foreground">
                <DynamicLucideIcon icon={GitBranchIcon} size={14} />
                <span className="truncate">{build.branch}</span>
              </span>
            ) : null}
            {build.commit_sha ? (
              <span className="inline-flex shrink-0 items-center gap-1 font-mono text-xs">
                <DynamicLucideIcon icon={GitCommitIcon} size={14} />
                {build.commit_sha.slice(0, 7)}
              </span>
            ) : null}
          </dd>
        </div>
      ) : null}

      <div className="flex items-center gap-1.5">
        <dt className="sr-only">Duration</dt>
        <DynamicLucideIcon icon={TimeQuarterPassIcon} size={14} aria-hidden />
        <dd>{duration != null ? formatDuration(duration) : 'Not started'}</dd>
      </div>

      <div>
        <dt className="sr-only">Timing</dt>
        <dd>{timingLabel}</dd>
      </div>

      {build.context?.runner_name ? (
        <div>
          <dt className="sr-only">Runner</dt>
          <dd>Runner {build.context.runner_name}</dd>
        </div>
      ) : null}

      {build.source_build_id ? (
        <div>
          <dt className="sr-only">Source build</dt>
          <dd>
            <Link
              to="/builds/$buildId"
              params={{ buildId: build.source_build_id }}
              className="underline underline-offset-4 hover:text-foreground"
            >
              Re-run of a previous build
            </Link>
          </dd>
        </div>
      ) : null}
    </dl>
  )
}
