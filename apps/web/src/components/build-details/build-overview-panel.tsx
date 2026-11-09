import type { ReactNode } from 'react'
import { Link } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import { Download04Icon } from '@hugeicons/core-free-icons'
import type {
  AnalyzerCounts,
  Artifact,
  Build,
  BuildEvent,
  BuildFeedback,
  BuildSizeReport,
  BuildTestSummary,
  Release,
} from '@oore/client/models'

import ChangelogMarkdown from './changelog-markdown'
import { EventTimeline } from './event-timeline'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Row } from '@/components/layout/row'
import { Stack } from '@/components/layout/stack'
import { Text } from '@/components/layout/text'
import {
  artifactPurposeLabel,
  latestAppPerPlatform,
} from '@/lib/artifact-install'
import { analyzerTabCount } from '@/lib/analyzer-format'
import { buildSourceRef } from '@/lib/build-source'
import { buildVersionLabel } from '@/lib/build-identity'
import { retryStepLabel } from '@/lib/build-steps'
import { commitUrl } from '@/lib/commit-url'
import {
  formatDuration,
  formatEnumLabel,
  formatFileSize,
  formatSizeChange,
} from '@/lib/format-utils'
import {
  releaseDestinationLabel,
  releaseStateBadgeVariant,
  releaseStateLabel,
} from '@/lib/releases'
import { TRIGGER_TYPE_LABELS } from '@/lib/status-variants'
import { textVariants } from '@/lib/recipes'
import { cn } from '@/lib/utils'

function clock(epochSecs: number) {
  return new Date(epochSecs * 1000).toLocaleString(undefined, {
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  })
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle>{title}</CardTitle>
      </CardHeader>
      <CardContent>{children}</CardContent>
    </Card>
  )
}

function Fact({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="grid grid-cols-[7.5rem_minmax(0,1fr)] gap-3 py-1.5">
      <dt className={textVariants({ variant: 'muted' })}>{label}</dt>
      <dd className="min-w-0 wrap-break-word">{children}</dd>
    </div>
  )
}

const LINK_CLASS = 'underline underline-offset-4 hover:text-foreground'

/** Queued, started and finished, with the wait and run time between them. */
export function BuildMoments({ build }: { build: Build }) {
  const moments = [
    { label: 'Queued', at: build.queued_at, gap: null },
    ...(build.approval?.decided_at
      ? [
          {
            label:
              build.approval.state === 'rejected' ? 'Rejected' : 'Approved',
            at: build.approval.decided_at,
            gap: null,
          },
        ]
      : []),
    ...(build.started_at
      ? [
          {
            label: 'Started',
            at: build.started_at,
            gap: `waited ${formatDuration(Math.max(0, build.started_at - build.queued_at))}`,
          },
        ]
      : []),
    ...(build.finished_at
      ? [
          {
            label: formatEnumLabel(build.status),
            at: build.finished_at,
            gap: build.started_at
              ? `took ${formatDuration(build.finished_at - build.started_at)}`
              : null,
          },
        ]
      : []),
  ]
  return (
    <ol
      aria-label="When it ran"
      className="flex flex-wrap items-start gap-x-6 gap-y-2"
    >
      {moments.map((moment) => (
        <li key={moment.label} className="min-w-0">
          <Text variant="caption">
            {moment.label}
            {moment.gap ? ` · ${moment.gap}` : ''}
          </Text>
          <span className="text-sm tabular-nums">{clock(moment.at)}</span>
        </li>
      ))}
    </ol>
  )
}

function AppsSection({
  build,
  artifacts,
  sizes,
  onOpenArtifacts,
}: {
  build: Build
  artifacts: Array<Artifact>
  sizes?: BuildSizeReport
  onOpenArtifacts: () => void
}) {
  const apps = latestAppPerPlatform(artifacts)
  const changes = new Map(
    (sizes?.artifacts ?? []).map((comparison) => [
      comparison.artifact_id,
      comparison,
    ]),
  )
  return (
    <Section title="Apps">
      {apps.length === 0 ? (
        <Text variant="muted">
          {artifacts.length
            ? 'Nothing here installs on a device. '
            : 'This build made no apps. '}
          {artifacts.length ? (
            <Button
              variant="link"
              size="xs"
              className="h-auto px-0 align-baseline"
              onClick={onOpenArtifacts}
            >
              See its files
            </Button>
          ) : null}
        </Text>
      ) : (
        <Stack gap={3}>
          {apps.map(({ artifact }) => {
            const change = changes.get(artifact.id)
            return (
              <Row key={artifact.id} gap={3} className="justify-between">
                <div className="min-w-0">
                  <p className="font-medium">
                    {artifactPurposeLabel(artifact)}
                  </p>
                  <Text variant="caption" className="truncate">
                    {artifact.name}
                    {artifact.file_size
                      ? ` · ${formatFileSize(artifact.file_size)}`
                      : ''}
                    {change?.delta_bytes != null && change.baseline_size_bytes
                      ? ` · ${formatSizeChange(change.delta_bytes, change.baseline_size_bytes)}`
                      : ''}
                  </Text>
                </div>
                <Button
                  size="sm"
                  variant="outline"
                  render={
                    <Link
                      to="/builds/$buildId"
                      params={{ buildId: build.id }}
                      search={{ install: artifact.id }}
                    />
                  }
                  nativeButton={false}
                >
                  <HugeiconsIcon icon={Download04Icon} size={14} />
                  Install
                </Button>
              </Row>
            )
          })}
          {sizes?.baseline ? (
            <Text variant="caption">
              Sizes compared with{' '}
              <Link
                to="/builds/$buildId"
                params={{ buildId: sizes.baseline.build_id }}
                className={LINK_CLASS}
              >
                #{sizes.baseline.build_number}
              </Link>{' '}
              on {sizes.baseline.branch}.
            </Text>
          ) : null}
        </Stack>
      )}
    </Section>
  )
}

function StoresSection({
  releases,
  onOpenRelease,
}: {
  releases: Array<Release>
  onOpenRelease: () => void
}) {
  return (
    <Section title="Sent to">
      {releases.length === 0 ? (
        <Text variant="muted">Not sent to a store.</Text>
      ) : (
        <Stack gap={2}>
          {releases.map((release) => (
            <Row key={release.id} gap={3} className="justify-between">
              <span className="min-w-0 truncate">
                {releaseDestinationLabel(release)}
              </span>
              <Badge variant={releaseStateBadgeVariant(release)}>
                {releaseStateLabel(release)}
              </Badge>
            </Row>
          ))}
          <Button
            variant="link"
            size="xs"
            className="h-auto self-start px-0"
            onClick={onOpenRelease}
          >
            Open the release
          </Button>
        </Stack>
      )}
    </Section>
  )
}

function ChecksSection({
  tests,
  analysis,
  onOpenTests,
  onOpenAnalyzer,
}: {
  tests?: BuildTestSummary
  analysis?: AnalyzerCounts
  onOpenTests: () => void
  onOpenAnalyzer: () => void
}) {
  if (!tests && !analysis) return null
  const analyzerCount = analysis ? analyzerTabCount(analysis) : null
  return (
    <Section title="Checks">
      <dl>
        {tests ? (
          <Fact label="Tests">
            <Button
              variant="link"
              size="xs"
              className={cn(
                'h-auto px-0 align-baseline',
                tests.failed ? 'text-destructive' : 'text-foreground',
              )}
              onClick={onOpenTests}
            >
              {tests.failed
                ? `${tests.failed} of ${tests.total} failed`
                : `${tests.passed} passed`}
            </Button>
            {tests.skipped ? (
              <Text variant="caption" render={<span />}>
                {' '}
                · {tests.skipped} skipped
              </Text>
            ) : null}
          </Fact>
        ) : null}
        {analysis ? (
          <Fact label="Analyzer">
            <Button
              variant="link"
              size="xs"
              className="h-auto px-0 align-baseline text-foreground"
              onClick={onOpenAnalyzer}
            >
              {analyzerCount ?? 'No errors or warnings'}
            </Button>
          </Fact>
        ) : null}
      </dl>
    </Section>
  )
}

function DetailsSection({ build }: { build: Build }) {
  const sourceRef = buildSourceRef(build)
  const version = buildVersionLabel(build)
  const commitHref = build.commit_sha
    ? commitUrl(build.context, build.commit_sha)
    : null
  return (
    <Section title="Details">
      <Stack gap={4}>
        <BuildMoments build={build} />
        <dl className="divide-y">
          {version ? <Fact label="Version">{version}</Fact> : null}
          {build.commit_sha ? (
            <Fact label="Commit">
              {commitHref ? (
                <a
                  href={commitHref}
                  target="_blank"
                  rel="noopener noreferrer"
                  className={cn('font-mono text-xs', LINK_CLASS)}
                >
                  {build.commit_sha.slice(0, 7)}
                </a>
              ) : (
                <span className="font-mono text-xs">
                  {build.commit_sha.slice(0, 7)}
                </span>
              )}
              {build.commit_title ? ` ${build.commit_title}` : ''}
            </Fact>
          ) : null}
          {build.commit_author ? (
            <Fact label="Author">{build.commit_author}</Fact>
          ) : null}
          {sourceRef ? (
            <Fact label={build.branch ? 'Branch' : 'Tag'}>
              <span className="font-mono text-xs">{sourceRef}</span>
            </Fact>
          ) : null}
          <Fact label="Started by">
            {TRIGGER_TYPE_LABELS[build.trigger_type]}
            {build.trigger_event
              ? ` · ${formatEnumLabel(build.trigger_event).toLowerCase()}`
              : ''}
            {build.trigger_actor ? ` · ${build.trigger_actor}` : ''}
          </Fact>
          {build.context?.runner_name ? (
            <Fact label="Runner">{build.context.runner_name}</Fact>
          ) : null}
          {build.source_build_id ? (
            <Fact label={build.retry_steps ? 'Retries' : 'Re-run of'}>
              <Link
                to="/builds/$buildId"
                params={{ buildId: build.source_build_id }}
                className={LINK_CLASS}
              >
                {build.retry_steps
                  ? `The ${build.retry_steps.map(retryStepLabel).join(' and ')} of a previous build`
                  : 'A previous build'}
              </Link>
            </Fact>
          ) : null}
          {build.superseded_by ? (
            <Fact label="Replaced by">
              <Link
                to="/builds/$buildId"
                params={{ buildId: build.superseded_by.id }}
                className={LINK_CLASS}
              >
                #{build.superseded_by.build_number}
              </Link>
            </Fact>
          ) : null}
        </dl>
      </Stack>
    </Section>
  )
}

/**
 * A build at a glance: the apps it made with Install, where they were
 * sent, what changed, its checks, its details and its activity.
 */
export function BuildOverviewPanel({
  build,
  events,
  feedback,
  artifacts,
  releases,
  sizes,
  tests,
  analysis,
  onOpenTab,
}: {
  build: Build
  events: Array<BuildEvent>
  feedback: Array<BuildFeedback>
  artifacts: Array<Artifact>
  releases: Array<Release>
  sizes?: BuildSizeReport
  tests?: BuildTestSummary
  analysis?: AnalyzerCounts
  onOpenTab: (tab: 'artifacts' | 'release' | 'tests' | 'analyzer') => void
}) {
  const finished = build.status === 'succeeded'
  return (
    <div className="grid items-start gap-4 lg:grid-cols-[minmax(0,1.2fr)_minmax(0,1fr)]">
      <Stack gap={4}>
        {finished ? (
          <AppsSection
            build={build}
            artifacts={artifacts}
            sizes={sizes}
            onOpenArtifacts={() => onOpenTab('artifacts')}
          />
        ) : null}
        {finished || releases.length ? (
          <StoresSection
            releases={releases}
            onOpenRelease={() => onOpenTab('release')}
          />
        ) : null}
        <Section title="What changed">
          {build.changelog ? (
            <ChangelogMarkdown>{build.changelog}</ChangelogMarkdown>
          ) : (
            <Text variant="muted">
              {build.commit_title ?? 'No release notes for this build.'}
            </Text>
          )}
        </Section>
        {build.store_notes ? (
          <Section title="What's new, for the stores">
            <p className="text-sm whitespace-pre-line">{build.store_notes}</p>
          </Section>
        ) : null}
        <EventTimeline events={events} feedback={feedback} />
      </Stack>
      <Stack gap={4}>
        <DetailsSection build={build} />
        <ChecksSection
          tests={tests}
          analysis={analysis}
          onOpenTests={() => onOpenTab('tests')}
          onOpenAnalyzer={() => onOpenTab('analyzer')}
        />
      </Stack>
    </div>
  )
}
