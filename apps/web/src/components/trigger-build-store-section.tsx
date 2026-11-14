import { HugeiconsIcon } from '@hugeicons/react'
import {
  AlertCircleIcon,
  CheckmarkCircle02Icon,
} from '@hugeicons/core-free-icons'
import type { UseFormReturn } from 'react-hook-form'
import type { StoreReadinessItem } from '@oore/client/models'
import { Button } from '@/components/ui/button'
import {
  FormControl,
  FormDescription,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
} from '@/components/ui/form'
import { Skeleton } from '@/components/ui/skeleton'
import { Textarea } from '@/components/ui/textarea'
import { Row } from '@/components/layout/row'
import { Stack } from '@/components/layout/stack'
import { Text } from '@/components/layout/text'
import { useBuilds } from '@/hooks/use-builds'
import { usePipelinePublishReadiness } from '@/hooks/use-pipeline-publish'
import { cn } from '@/lib/utils'
import type { TriggerBuildForm } from '@/components/trigger-build-form'

/** Longest release notes Google Play takes per language. */
const PLAY_NOTES_LIMIT = 500

/**
 * Drafted changelog markdown ("- Fix login — Mina") as notes people read
 * in a store: one line per change, without authors or markdown.
 */
export function storeNotesFromChangelog(markdown: string | undefined) {
  if (!markdown) return ''
  return markdown
    .split('\n')
    .map((line) => line.trim())
    .filter((line) => line.startsWith('- ') && !line.startsWith('- …'))
    .map((line) =>
      line
        .slice(2)
        .replace(/ — [^—]*$/, '')
        .replace(/\\(.)/g, '$1')
        // "feat(login): faster sign in" reads as "Faster sign in".
        .replace(/^[a-z]+(\([^)]*\))?!?:\s*/, ''),
    )
    .filter(Boolean)
    .map((title) => `• ${title.charAt(0).toUpperCase()}${title.slice(1)}`)
    .join('\n')
}

const STORE_NAME = {
  app_store: 'App Store',
  google_play: 'Google Play',
} satisfies Record<StoreReadinessItem['store'], string>

/**
 * For a build that uploads to a store: what would stop the submission, so
 * it shows before a long build rather than after, and the release notes
 * for this release.
 */
export function TriggerBuildStoreSection({
  form,
  pipelineId,
  changelogMarkdown,
  enabled,
  filePublishes,
}: {
  form: UseFormReturn<TriggerBuildForm>
  pipelineId: string
  changelogMarkdown: string | undefined
  enabled: boolean
  /** Store uploads the chosen workflow's file names. */
  filePublishes: Array<string>
}) {
  const readiness = usePipelinePublishReadiness(pipelineId, {
    enabled: enabled && !!pipelineId,
  })
  // The last notes someone wrote for this pipeline, to start from.
  const lastNotesQuery = useBuilds(
    { pipeline_id: pipelineId, limit: 20 },
    {
      enabled: enabled && !!pipelineId,
      select: (data) =>
        data.builds.find((build) => build.store_notes?.trim()) ?? null,
    },
  )
  const items = readiness.data?.items ?? []
  const uploads = items.length > 0 || filePublishes.length > 0
  if (!pipelineId || (!uploads && !readiness.isPending)) return null
  if (readiness.isPending && filePublishes.length === 0) return null

  const lastNotes = lastNotesQuery.data
  const drafted = storeNotesFromChangelog(changelogMarkdown)

  return (
    <Stack gap={4} className="rounded-lg border p-4">
      <StoreReadinessChecklist readiness={readiness} />

      <FormField
        control={form.control}
        name="store_notes"
        render={({ field }) => (
          <FormItem>
            <FormLabel>What's new in this release</FormLabel>
            <FormControl>
              <Textarea
                {...field}
                value={field.value ?? ''}
                rows={4}
                placeholder="Leave empty to use the release notes in the pipeline's publish settings."
              />
            </FormControl>
            <Row gap={1} className="flex-wrap">
              {drafted ? (
                <Button
                  type="button"
                  variant="ghost"
                  size="xs"
                  onClick={() =>
                    form.setValue('store_notes', drafted, {
                      shouldDirty: true,
                    })
                  }
                >
                  Draft from commits
                </Button>
              ) : null}
              {lastNotes?.store_notes ? (
                <Button
                  type="button"
                  variant="ghost"
                  size="xs"
                  onClick={() =>
                    form.setValue('store_notes', lastNotes.store_notes ?? '', {
                      shouldDirty: true,
                    })
                  }
                >
                  Same as #{lastNotes.build_number}
                </Button>
              ) : null}
            </Row>
            <FormDescription>
              Testers and store users read this. It replaces the pipeline's
              notes for this build: Google Play shows the first{' '}
              {PLAY_NOTES_LIMIT} characters.
            </FormDescription>
            <FormMessage />
          </FormItem>
        )}
      />
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
}: {
  readiness: ReturnType<typeof usePipelinePublishReadiness>
  title?: string
}) {
  const items = readiness.data?.items ?? []
  const problems = items.filter((item) => item.level !== 'ok')
  return (
    <Stack gap={2} role="group" aria-label="Ready to submit?">
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
          {readiness.data?.preflight === 'warn'
            ? 'The upload goes ahead, but the store will refuse it until these are fixed.'
            : 'Fix these first, or the publish step stops before the upload.'}
        </Text>
      ) : null}
    </Stack>
  )
}
