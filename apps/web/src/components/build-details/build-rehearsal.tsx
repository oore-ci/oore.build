import { lazy, Suspense, useMemo, useState } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  AlertCircleIcon,
  CheckmarkCircle02Icon,
} from '@hugeicons/core-free-icons'
import type { Build, StoreReadinessItem } from '@oore/client/models'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Skeleton } from '@/components/ui/skeleton'
import { Spinner } from '@/components/ui/spinner'
import { Row } from '@/components/layout/row'
import { Stack } from '@/components/layout/stack'
import { Text } from '@/components/layout/text'
import { useBuildRehearsal } from '@/hooks/use-build-rehearsal'
import { cn } from '@/lib/utils'

const TriggerBuildDrawer = lazy(
  () => import('@/components/trigger-build-drawer'),
)

const STORE_NAME = {
  app_store: 'App Store',
  google_play: 'Google Play',
} satisfies Record<StoreReadinessItem['store'], string>

const CONSOLE_NAME = {
  app_store: 'Fix in App Store Connect',
  google_play: 'Fix in Play Console',
} satisfies Record<StoreReadinessItem['store'], string>

/** The badge beside a rehearsal build's status, so nobody takes it for a release. */
export function RehearsalBadge({ className }: { className?: string }) {
  return (
    <Badge variant="outline" className={className}>
      Rehearsal · nothing submitted
    </Badge>
  )
}

function CheckRow({ item }: { item: StoreReadinessItem }) {
  return (
    <li className="flex items-start gap-2 py-2 text-sm">
      <HugeiconsIcon
        icon={item.level === 'ok' ? CheckmarkCircle02Icon : AlertCircleIcon}
        className={cn(
          'mt-0.5 size-4 shrink-0',
          item.level === 'ok'
            ? 'text-success'
            : item.level === 'block'
              ? 'text-destructive'
              : 'text-warning',
        )}
        aria-hidden
      />
      <Stack gap={0.5} className="min-w-0 flex-1">
        <span className={cn(item.level !== 'ok' && 'font-medium')}>
          <span className="sr-only">
            {item.level === 'ok'
              ? 'Passed: '
              : item.level === 'block'
                ? 'Fix first: '
                : 'Check: '}
          </span>
          {STORE_NAME[item.store]}: {item.title}
        </span>
        {item.detail ? (
          <span className="wrap-break-word text-muted-foreground">
            {item.detail}
          </span>
        ) : null}
      </Stack>
      {item.console_url && item.level !== 'ok' ? (
        <Button
          variant="outline"
          size="xs"
          nativeButton={false}
          render={
            <a href={item.console_url} target="_blank" rel="noreferrer" />
          }
        >
          {CONSOLE_NAME[item.store]}
        </Button>
      ) : null}
    </li>
  )
}

/**
 * What a release rehearsal found: a pass, or the fixes to make first, each
 * with the store page that fixes it. A pass offers the real release with the
 * same pipeline, workflow and commit.
 */
export function BuildRehearsalCard({
  build,
  canRelease,
}: {
  build: Build
  canRelease: boolean
}) {
  const finished = !['queued', 'assigned', 'running'].includes(build.status)
  const query = useBuildRehearsal(build.id, !finished)
  const [releaseOpen, setReleaseOpen] = useState(false)
  const prefill = useMemo(
    () => ({
      project_id: build.project_id,
      pipeline_id: build.pipeline_id,
      branch: build.branch ?? '',
      commit_sha: build.commit_sha ?? '',
      workflow: build.workflow ?? '',
      store_notes: build.store_notes ?? '',
    }),
    [build],
  )
  const checks = query.data?.checks ?? []
  const stores = query.data?.stores ?? []
  const blocking = checks.filter((check) => check.level === 'block').length
  const passed =
    finished &&
    build.status === 'succeeded' &&
    stores.length > 0 &&
    blocking === 0

  const title = !finished
    ? 'Rehearsing the release'
    : blocking > 0
      ? `Fix ${blocking} ${blocking === 1 ? 'thing' : 'things'} before you release`
      : passed
        ? 'Ready to release'
        : stores.length === 0
          ? 'No store was checked'
          : 'The rehearsal did not finish'
  const summary = !finished
    ? 'Oore builds and signs as the release would, then asks each store to check it. Nothing is submitted.'
    : blocking > 0
      ? 'The stores would refuse this release as it is. Fix these, then rehearse again with Re-run.'
      : passed
        ? `${stores.map((store) => STORE_NAME[store]).join(' and ')} would accept this release. Nothing was submitted.`
        : stores.length === 0
          ? build.status === 'succeeded'
            ? 'This build publishes to no store, so there was nothing to rehearse.'
            : 'The build stopped before the store checks. Its log shows why.'
          : 'Some store checks did not run. The log shows why.'

  return (
    <Card size="sm" aria-label="Release rehearsal">
      <CardHeader>
        <Row gap={2} className="justify-between">
          <CardTitle className="flex items-center gap-2">
            {!finished ? <Spinner className="size-4" /> : null}
            {title}
          </CardTitle>
          {passed && canRelease ? (
            <Button size="sm" onClick={() => setReleaseOpen(true)}>
              Release this commit
            </Button>
          ) : null}
        </Row>
      </CardHeader>
      <CardContent>
        <Stack gap={2}>
          <Text variant="muted">{summary}</Text>
          {query.isPending ? (
            <Skeleton className="h-16 w-full" />
          ) : query.error ? (
            <Text variant="muted">
              The rehearsal findings could not be loaded.
            </Text>
          ) : checks.length ? (
            <ul className="divide-y" aria-label="Rehearsal checks">
              {checks.map((item) => (
                <CheckRow key={`${item.store}-${item.code}`} item={item} />
              ))}
            </ul>
          ) : null}
        </Stack>
      </CardContent>
      {releaseOpen ? (
        <Suspense fallback={null}>
          <TriggerBuildDrawer
            open
            onOpenChange={setReleaseOpen}
            fixedProjectId={build.project_id}
            prefill={prefill}
            title="Release this commit"
            description="The same pipeline, workflow and commit as the rehearsal, this time submitted to the stores."
          />
        </Suspense>
      ) : null}
    </Card>
  )
}
