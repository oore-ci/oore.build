import { HugeiconsIcon } from '@hugeicons/react'
import {
  AlertCircleIcon,
  CheckmarkCircle02Icon,
} from '@hugeicons/core-free-icons'
import type {
  StoreReadinessItem,
  StoreReadinessResponse,
} from '@oore/client/models'
import { Button } from '@/components/ui/button'
import { Skeleton } from '@/components/ui/skeleton'
import { Row } from '@/components/layout/row'
import { Stack } from '@/components/layout/stack'
import { Text } from '@/components/layout/text'
import {
  usePipelineAppStorePublish,
  usePipelineGooglePlayPublish,
  usePipelinePublishReadiness,
} from '@/hooks/use-pipeline-publish'
import { describeStoreUploads } from '@/lib/store-destination'
import type { BuildPlatform } from '@oore/client/models'
import { cn } from '@/lib/utils'

const STORE_NAME = {
  app_store: 'App Store',
  google_play: 'Google Play',
} satisfies Record<StoreReadinessItem['store'], string>

/**
 * Where this build goes, in one sentence at the top of Run build: the
 * picked workflow's uploads when its file names some, otherwise the
 * pipeline's publishing settings.
 */
export function TriggerBuildDestination({
  enabled,
  filePublishes,
  pipelineId,
  platforms,
}: {
  enabled: boolean
  filePublishes: Array<string>
  pipelineId: string
  platforms: ReadonlyArray<BuildPlatform>
}) {
  const android = platforms.includes('android')
  const ios = platforms.includes('ios')
  const playQuery = usePipelineGooglePlayPublish(pipelineId, {
    enabled: enabled && android && filePublishes.length === 0,
  })
  const appStoreQuery = usePipelineAppStorePublish(pipelineId, {
    enabled: enabled && ios && filePublishes.length === 0,
  })
  let sentence: string | null
  if (filePublishes.length > 0) {
    // "android publishes to … (workflows.release…)" as a sentence.
    sentence = filePublishes
      .map((line) => line.replace(/\s*\([^)]*\)$/, ''))
      .map((line) => `${line.charAt(0).toUpperCase()}${line.slice(1)}.`)
      .join(' ')
  } else if ((android && !playQuery.data) || (ios && !appStoreQuery.data)) {
    sentence = null
  } else {
    sentence = describeStoreUploads({
      googlePlay: android ? playQuery.data?.config : null,
      appStore: ios ? appStoreQuery.data?.config : null,
    })
  }
  if (!sentence) return <Skeleton className="h-5 w-64" />
  return (
    <p className="text-sm" aria-live="polite">
      {sentence}
    </p>
  )
}

/**
 * For a build that uploads to a store: what would stop the submission, so
 * it shows before a long build rather than after, and the release notes
 * for this release.
 */
export function TriggerBuildStoreSection({
  pipelineId,
  enabled,
  filePublishes,
}: {
  pipelineId: string
  enabled: boolean
  /** Store uploads the chosen workflow's file names. */
  filePublishes: Array<string>
}) {
  const readiness = usePipelinePublishReadiness(pipelineId, {
    enabled: enabled && !!pipelineId,
  })
  const items = readiness.data?.items ?? []
  const uploads = items.length > 0 || filePublishes.length > 0
  if (!pipelineId || (!uploads && !readiness.isPending)) return null
  if (readiness.isPending && filePublishes.length === 0) return null

  return (
    <Stack gap={4} className="rounded-lg border p-4">
      <StoreReadinessChecklist readiness={readiness} />
      <Stack gap={1}>
        <Text className="font-medium">Release notes</Text>
        <Text variant="muted">
          The stores get the release notes of the app version this build makes.
          Oore drafts them from commits, and you can edit them on the build page
          once the build starts.
        </Text>
      </Stack>
    </Stack>
  )
}

/**
 * What would stop the pipeline's next store submission, problems only, each
 * with the console page that fixes it.
 */
export function StoreReadinessChecklist({
  readiness,
  title = 'Ready to submit?',
  blockingNote,
}: {
  readiness: {
    data?: StoreReadinessResponse
    isPending: boolean
    error: unknown
  }
  title?: string
  /** Replaces the note under blocking findings. */
  blockingNote?: string
}) {
  const items = readiness.data?.items ?? []
  const problems = items.filter((item) => item.level !== 'ok')
  return (
    <Stack gap={2} role="group" aria-label={title}>
      <p className="text-sm font-medium">{title}</p>
      {readiness.isPending ? (
        <Skeleton className="h-10 w-full" />
      ) : readiness.error ? (
        <Text variant="muted">
          The store checks could not run. The upload checks again before it
          starts.
        </Text>
      ) : problems.length === 0 ? (
        <Row gap={2} className="text-sm">
          <HugeiconsIcon
            icon={CheckmarkCircle02Icon}
            className="size-4 text-success"
            aria-hidden
          />
          <span>
            {items.length
              ? 'The store checks pass.'
              : 'This workflow uploads to a store.'}
          </span>
        </Row>
      ) : (
        <ul className="flex flex-col gap-2">
          {problems.map((item) => (
            <li
              key={`${item.store}-${item.code}`}
              className="flex items-start gap-2 text-sm"
            >
              <HugeiconsIcon
                icon={AlertCircleIcon}
                className={cn(
                  'mt-0.5 size-4 shrink-0',
                  item.level === 'block' ? 'text-destructive' : 'text-warning',
                )}
                aria-hidden
              />
              <Stack gap={0.5} className="min-w-0 flex-1">
                <span className="font-medium">
                  {STORE_NAME[item.store]}: {item.title}
                </span>
                {item.detail ? (
                  <span className="text-muted-foreground">{item.detail}</span>
                ) : null}
              </Stack>
              {item.console_url ? (
                <Button
                  variant="outline"
                  size="xs"
                  nativeButton={false}
                  render={
                    <a
                      href={item.console_url}
                      target="_blank"
                      rel="noreferrer"
                    />
                  }
                >
                  Fix
                </Button>
              ) : null}
            </li>
          ))}
        </ul>
      )}
      {problems.some((item) => item.level === 'block') ? (
        <Text variant="caption">
          {blockingNote ??
            (readiness.data?.preflight === 'warn'
              ? 'The upload goes ahead, but the store will refuse it until these are fixed.'
              : 'Fix these first, or the publish step stops before the upload.')}
        </Text>
      ) : null}
    </Stack>
  )
}
