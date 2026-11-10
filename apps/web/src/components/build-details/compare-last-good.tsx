import { useState } from 'react'
import { Link } from '@tanstack/react-router'
import type { Build, StepResult } from '@oore/client/models'

import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Text } from '@/components/layout/text'
import { buildVersionLabel } from '@/lib/build-identity'
import { buildStepLabel } from '@/lib/build-steps'
import { formatDuration, relativeTime } from '@/lib/format-utils'
import { cn } from '@/lib/utils'

/**
 * The last succeeded build to compare a failed one with: on the same branch
 * when there is one, else the newest.
 */
export function lastGoodBuild(
  build: Build,
  succeeded: ReadonlyArray<Build>,
): Build | null {
  const earlier = succeeded.filter(
    (candidate) =>
      candidate.id !== build.id &&
      candidate.status === 'succeeded' &&
      candidate.created_at < build.created_at,
  )
  return (
    earlier.find((candidate) => candidate.branch === build.branch) ??
    earlier[0] ??
    null
  )
}

type Row = { label: string; good: string; failed: string }

function factRows(good: Build, failed: Build): Array<Row> {
  const commit = (b: Build) =>
    b.commit_sha
      ? `${b.commit_sha.slice(0, 7)}${b.commit_title ? ` ${b.commit_title}` : ''}`
      : '—'
  return [
    { label: 'Commit', good: commit(good), failed: commit(failed) },
    {
      label: 'Branch',
      good: good.branch ?? '—',
      failed: failed.branch ?? '—',
    },
    {
      label: 'Version',
      good: buildVersionLabel(good) ?? '—',
      failed: buildVersionLabel(failed) ?? '—',
    },
    {
      label: 'Runner',
      good: good.context?.runner_name ?? '—',
      failed: failed.context?.runner_name ?? '—',
    },
    {
      label: 'Workflow',
      good: good.workflow ?? 'Default',
      failed: failed.workflow ?? 'Default',
    },
  ]
}

function stepRows(good: Build, failed: Build): Array<Row> {
  const goodSteps = good.step_results ?? []
  const failedSteps = failed.step_results ?? []
  const names = [
    ...new Set([...goodSteps, ...failedSteps].map((step) => step.name)),
  ]
  const time = (steps: ReadonlyArray<StepResult>, name: string) => {
    const step = steps.find((candidate) => candidate.name === name)
    if (!step) return 'Not run'
    const took = formatDuration(step.duration_ms / 1000)
    return step.status === 'succeeded' ? took : `${step.status}, ${took}`
  }
  return names.map((name) => ({
    label: buildStepLabel(name, names),
    good: time(goodSteps, name),
    failed: time(failedSteps, name),
  }))
}

function CompareTable({
  caption,
  rows,
  good,
  failed,
}: {
  caption: string
  rows: Array<Row>
  good: number
  failed: number
}) {
  return (
    <table className="w-full table-fixed text-sm">
      <caption className="pb-1 text-left text-xs font-medium text-muted-foreground">
        {caption}
      </caption>
      <thead>
        <tr className="text-left text-xs text-muted-foreground">
          <th className="w-1/4 pb-1 font-normal">
            <span className="sr-only">Field</span>
          </th>
          <th className="pb-1 font-normal">#{good}</th>
          <th className="pb-1 font-normal">#{failed}</th>
        </tr>
      </thead>
      <tbody className="divide-y">
        {rows.map((row) => {
          const changed = row.good !== row.failed
          return (
            <tr key={row.label} className="align-top">
              <th
                scope="row"
                className="w-1/4 py-1.5 pr-3 text-left font-normal text-muted-foreground"
              >
                {row.label}
              </th>
              <td className="py-1.5 pr-3 wrap-break-word">{row.good}</td>
              <td
                className={cn(
                  'py-1.5 wrap-break-word',
                  changed && 'font-medium',
                )}
              >
                {row.failed}
              </td>
            </tr>
          )
        })}
      </tbody>
    </table>
  )
}

/** Side by side with the last good build: what differs, step by step. */
export function CompareWithLastGood({
  build,
  good,
}: {
  build: Build
  good: Build
}) {
  const [open, setOpen] = useState(false)
  return (
    <>
      <Button
        variant="link"
        size="xs"
        className="h-auto px-0 align-baseline text-muted-foreground underline"
        onClick={() => setOpen(true)}
      >
        Compare with the last good build
      </Button>
      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent className="sm:max-w-2xl">
          <DialogHeader>
            <DialogTitle>
              #{good.build_number} worked, #{build.build_number} failed
            </DialogTitle>
            <DialogDescription>
              Build #{good.build_number} succeeded{' '}
              {relativeTime(good.finished_at ?? good.created_at)}. What differs
              is in bold.
            </DialogDescription>
          </DialogHeader>
          <CompareTable
            caption="The build"
            rows={factRows(good, build)}
            good={good.build_number}
            failed={build.build_number}
          />
          {good.step_results?.length && build.step_results?.length ? (
            <CompareTable
              caption="Steps"
              rows={stepRows(good, build)}
              good={good.build_number}
              failed={build.build_number}
            />
          ) : null}
          <Text variant="caption">
            <Link
              to="/builds/$buildId"
              params={{ buildId: good.id }}
              className="underline underline-offset-4"
            >
              Open build #{good.build_number}
            </Link>
          </Text>
        </DialogContent>
      </Dialog>
    </>
  )
}
