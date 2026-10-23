import { Link } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  GitBranchIcon,
  GitCommitIcon,
  TimeQuarterPassIcon,
} from '@hugeicons/core-free-icons'

import type { Build } from '@oore/client/models'
import { retryStepLabel } from '@/lib/build-steps'
import { commitUrl } from '@/lib/commit-url'
import { formatDuration, relativeTime } from '@/lib/format-utils'
import { Row } from '@/components/layout/row'
import { monoVariants } from '@/lib/recipes'
import { cn } from '@/lib/utils'

export function BuildSummary({
  build,
  duration,
}: {
  build: Build
  duration: number | null
}) {
  const timingLabel = build.finished_at
    ? `${build.status === 'expired' ? 'Expired' : build.status === 'canceled' && !build.started_at ? 'Canceled' : 'Finished'} ${relativeTime(build.finished_at)}`
    : build.started_at
      ? `Started ${relativeTime(build.started_at)}`
      : `Queued ${relativeTime(build.queued_at)}`
  const durationLabel =
    duration == null
      ? build.finished_at
        ? 'Never ran'
        : 'Not started'
      : build.finished_at
        ? `Took ${formatDuration(duration)}`
        : `Running for ${formatDuration(duration)}`
  const sourceRef = buildSourceRef(build)
  const commitHref = build.commit_sha
    ? commitUrl(build.context, build.commit_sha)
    : null

  return (
    <dl
      aria-label="Build summary"
      className="flex min-w-0 flex-wrap items-center gap-x-4 gap-y-2"
    >
      {sourceRef || build.commit_sha ? (
        <Row gap={2} className="min-w-0">
          <dt className="sr-only">Source</dt>
          <dd className="flex min-w-0 items-center gap-2">
            {sourceRef ? (
              <span className="inline-flex min-w-0 items-center gap-1 font-mono text-xs text-foreground">
                <HugeiconsIcon icon={GitBranchIcon} size={14} />
                <span className="truncate">{sourceRef}</span>
              </span>
            ) : null}
            {build.commit_sha && commitHref ? (
              <a
                href={commitHref}
                target="_blank"
                rel="noopener noreferrer"
                className={cn(
                  monoVariants({ size: 'xs' }),
                  'inline-flex shrink-0 items-center gap-1 underline-offset-4 hover:text-foreground hover:underline',
                )}
                title={`Open commit ${build.commit_sha} on the source host`}
              >
                <HugeiconsIcon icon={GitCommitIcon} size={14} />
                {build.commit_sha.slice(0, 7)}
              </a>
            ) : build.commit_sha ? (
              <span
                className="inline-flex shrink-0 items-center gap-1 font-mono text-xs"
                title={build.commit_sha}
              >
                <HugeiconsIcon icon={GitCommitIcon} size={14} />
                {build.commit_sha.slice(0, 7)}
              </span>
            ) : null}
          </dd>
        </Row>
      ) : null}

      <Row gap={1.5}>
        <dt className="sr-only">Duration</dt>
        <HugeiconsIcon icon={TimeQuarterPassIcon} size={14} aria-hidden />
        <dd>{durationLabel}</dd>
      </Row>

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
              {build.retry_steps
                ? `Retries the ${build.retry_steps.map(retryStepLabel).join(' and ')} of a previous build`
                : 'Re-run of a previous build'}
            </Link>
          </dd>
        </div>
      ) : null}
    </dl>
  )
}

/** Branch, or the tag or ref that triggered a build with no branch. */
export function buildSourceRef(build: Build): string | null {
  return (
    build.branch ??
    build.trigger_ref?.replace(/^refs\/(?:heads|tags)\//, '') ??
    null
  )
}
