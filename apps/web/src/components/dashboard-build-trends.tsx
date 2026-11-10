import { Link } from '@tanstack/react-router'
import type { FlakyTest, SlowerStep } from '@oore/client/models'

import {
  DashboardSection,
  DashboardSectionError,
} from '@/components/dashboard-section'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Skeleton } from '@/components/ui/skeleton'
import { Text } from '@/components/layout/text'
import { useBuildTrends } from '@/hooks/use-build-trends'
import type { BuildInsightsWindow } from '@/hooks/use-builds'
import { buildStepLabel } from '@/lib/build-steps'
import { formatDuration, relativeTime } from '@/lib/format-utils'

const LINK_CLASS = 'underline-offset-4 hover:underline'

function BuildLink({ id, number }: { id: string; number: number }) {
  return (
    <Link to="/builds/$buildId" params={{ buildId: id }} className={LINK_CLASS}>
      #{number}
    </Link>
  )
}

function FlakyRow({ test }: { test: FlakyTest }) {
  return (
    <li className="py-2">
      <p className="truncate text-sm font-medium" title={test.name}>
        {test.name}
      </p>
      <Text variant="caption">
        {test.project_name} · {test.pipeline_name} ·{' '}
        {test.occurrences === 1
          ? 'failed once, then passed on the same commit'
          : `failed then passed on the same commit ${test.occurrences} times`}
        , last{' '}
        <BuildLink
          id={test.failed_build_id}
          number={test.failed_build_number}
        />{' '}
        then{' '}
        <BuildLink
          id={test.passed_build_id}
          number={test.passed_build_number}
        />{' '}
        {relativeTime(test.last_seen_at)}
      </Text>
    </li>
  )
}

function SlowerRow({ step }: { step: SlowerStep }) {
  const percent = step.before_seconds
    ? Math.round(
        ((step.after_seconds - step.before_seconds) / step.before_seconds) *
          100,
      )
    : null
  return (
    <li className="py-2">
      <p className="text-sm">
        <span className="font-medium">{buildStepLabel(step.step)}</span>{' '}
        <span className="tabular-nums">
          {formatDuration(step.before_seconds)} →{' '}
          {formatDuration(step.after_seconds)}
          {percent != null ? ` (+${percent}%)` : ''}
        </span>
      </p>
      <Text variant="caption">
        {step.project_name} · {step.pipeline_name} · middle time in the first
        and second half of the period, from {step.builds} good builds
      </Text>
    </li>
  )
}

/**
 * What changed across builds over the period: tests that are flaky rather
 * than broken, and steps that got slower.
 */
export function DashboardBuildTrends({ days }: { days: BuildInsightsWindow }) {
  const trendsQuery = useBuildTrends(days)
  const trends = trendsQuery.data
  return (
    <DashboardSection title="Trends" scope={`Last ${days} days`}>
      {trendsQuery.error && !trends ? (
        <DashboardSectionError
          label="Trends"
          onRetry={() => void trendsQuery.refetch()}
        />
      ) : !trends ? (
        <Skeleton className="h-40 w-full" />
      ) : (
        <div className="grid items-start gap-4 lg:grid-cols-2">
          <Card size="sm">
            <CardHeader>
              <CardTitle>Flaky tests</CardTitle>
            </CardHeader>
            <CardContent>
              {trends.flaky_tests.length ? (
                <ul className="divide-y">
                  {trends.flaky_tests.map((test) => (
                    <FlakyRow
                      key={`${test.pipeline_id}:${test.suite}:${test.name}`}
                      test={test}
                    />
                  ))}
                </ul>
              ) : (
                <Text variant="muted">
                  No test failed and then passed on the same commit.
                </Text>
              )}
            </CardContent>
          </Card>
          <Card size="sm">
            <CardHeader>
              <CardTitle>Slower steps</CardTitle>
            </CardHeader>
            <CardContent>
              {trends.slower_steps.length ? (
                <ul className="divide-y">
                  {trends.slower_steps.map((step) => (
                    <SlowerRow
                      key={`${step.pipeline_id}:${step.step}`}
                      step={step}
                    />
                  ))}
                </ul>
              ) : (
                <Text variant="muted">No step got noticeably slower.</Text>
              )}
            </CardContent>
          </Card>
        </div>
      )}
    </DashboardSection>
  )
}
