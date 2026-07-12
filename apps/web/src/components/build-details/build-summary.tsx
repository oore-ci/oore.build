import { Link } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  GitBranchIcon,
  GitCommitIcon,
  TimeQuarterPassIcon,
} from '@hugeicons/core-free-icons'

import type { Build } from '@/lib/types'
import { Card, CardContent } from '@/components/ui/card'
import { formatDuration, relativeTime } from '@/lib/format-utils'

export function BuildSummary({
  build,
  duration,
}: {
  build: Build
  duration: number | null
}) {
  const hasContext =
    build.context?.project_name ||
    build.context?.pipeline_name ||
    build.context?.runner_name ||
    build.source_build_id

  return (
    <Card size="sm" aria-label="Build summary">
      <CardContent>
        <dl className="grid gap-x-6 gap-y-4 sm:grid-cols-2 xl:grid-cols-4">
          <div className="min-w-0 space-y-1">
            <dt className="text-xs font-medium text-muted-foreground">
              Source
            </dt>
            <dd className="flex min-w-0 items-center gap-2">
              {build.branch ? (
                <span className="inline-flex min-w-0 items-center gap-1.5 font-mono text-xs">
                  <HugeiconsIcon icon={GitBranchIcon} />
                  <span className="truncate">{build.branch}</span>
                </span>
              ) : (
                <span className="text-muted-foreground">Not recorded</span>
              )}
              {build.commit_sha ? (
                <span className="inline-flex shrink-0 items-center gap-1 font-mono text-xs text-muted-foreground">
                  <HugeiconsIcon icon={GitCommitIcon} />
                  {build.commit_sha.slice(0, 7)}
                </span>
              ) : null}
            </dd>
          </div>

          <div className="space-y-1">
            <dt className="text-xs font-medium text-muted-foreground">
              Duration
            </dt>
            <dd className="inline-flex items-center gap-1.5">
              <HugeiconsIcon icon={TimeQuarterPassIcon} />
              {duration != null ? formatDuration(duration) : 'Not started'}
            </dd>
          </div>

          <div className="space-y-1">
            <dt className="text-xs font-medium text-muted-foreground">
              Timeline
            </dt>
            <dd className="text-xs leading-5">
              <span>Queued {relativeTime(build.queued_at)}</span>
              {build.started_at ? (
                <span className="block text-muted-foreground">
                  Started {relativeTime(build.started_at)}
                </span>
              ) : null}
              {build.finished_at ? (
                <span className="block text-muted-foreground">
                  Finished {relativeTime(build.finished_at)}
                </span>
              ) : null}
            </dd>
          </div>

          <div className="space-y-1">
            <dt className="text-xs font-medium text-muted-foreground">
              Context
            </dt>
            <dd className="text-xs leading-5">
              {build.context?.project_name ? (
                <span className="block">{build.context.project_name}</span>
              ) : null}
              {build.context?.pipeline_name ? (
                <span className="block text-muted-foreground">
                  {build.context.pipeline_name}
                </span>
              ) : null}
              {build.context?.runner_name ? (
                <span className="block text-muted-foreground">
                  Runner: {build.context.runner_name}
                </span>
              ) : null}
              {build.source_build_id ? (
                <Link
                  to="/builds/$buildId"
                  params={{ buildId: build.source_build_id }}
                  className="text-muted-foreground underline underline-offset-4 hover:text-foreground"
                >
                  Re-run of a previous build
                </Link>
              ) : null}
              {!hasContext ? (
                <span className="text-muted-foreground">Not recorded</span>
              ) : null}
            </dd>
          </div>
        </dl>
      </CardContent>
    </Card>
  )
}
