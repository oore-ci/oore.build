import { useState } from 'react'
import {
  createLazyFileRoute,
  useNavigate,
  useSearch,
} from '@tanstack/react-router'
import { useForm } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import * as z from 'zod'
import { toast } from '@/lib/toast'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  Alert02Icon,
  InformationCircleIcon,
  Search01Icon,
  ServerStack01Icon,
} from '@hugeicons/core-free-icons'

import type { Runner, UpdateRunnerRequest } from '@oore/client/models'
import { useHasPermission } from '@/hooks/use-permissions'
import { usePageClamp } from '@/hooks/use-page-clamp'
import {
  usePurgeRunnerCaches,
  useRunners,
  useUpdateRunner,
} from '@/hooks/use-runners'
import { PageMeta } from '@/lib/seo'
import PageLayout from '@/components/page-layout'
import PageHeader from '@/components/page-header'
import ConfirmDialog from '@/components/ConfirmDialog'
import { DirectRunnerPolicyPanel } from '@/components/settings/direct-runner-policy-panel'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import {
  Form,
  FormControl,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
} from '@/components/ui/form'
import { Input } from '@/components/ui/input'
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from '@/components/ui/empty'
import { Spinner } from '@/components/ui/spinner'
import type { SortDirection } from '@/components/data-table-features'
import type { RunnerSort, RunnersSearch } from './runners'
import { RunnerInventory } from './-runner-inventory'
import { Text } from '@/components/layout/text'
import { splitRowVariants } from '@/lib/recipes'

export const Route = createLazyFileRoute('/settings/runners')({
  component: RunnersSettingsPage,
})

const LABEL_PATTERN = /^[a-z0-9._-]{1,64}$/

function parseLabels(value: string) {
  return [
    ...new Set(
      value
        .split(/[\s,]+/)
        .map((label) => label.trim().toLowerCase())
        .filter(Boolean),
    ),
  ].sort()
}

const editRunnerSchema = z.object({
  name: z
    .string()
    .trim()
    .min(1, 'Name is required')
    .max(255, 'Name must be at most 255 characters'),
  labels: z
    .string()
    .refine(
      (value) => parseLabels(value).every((label) => LABEL_PATTERN.test(label)),
      'Use letters, digits, ".", "_" and "-", separated by commas or spaces',
    )
    .refine((value) => parseLabels(value).length <= 32, 'At most 32 labels'),
})

type EditRunnerForm = z.infer<typeof editRunnerSchema>

interface EditRunnerDialogProps {
  runner: Runner
  onClose: () => void
}

function EditRunnerDialog({ runner, onClose }: EditRunnerDialogProps) {
  const mutation = useUpdateRunner()
  const initialLabels = runner.custom_labels ?? []
  const form = useForm<EditRunnerForm>({
    resolver: zodResolver(editRunnerSchema),
    defaultValues: { name: runner.name, labels: initialLabels.join(', ') },
    mode: 'onBlur',
  })

  const isManaged = !runner.registered_by
  const detectedLabels = (runner.labels ?? []).filter(
    (label) => !initialLabels.includes(label),
  )

  function onSubmit(data: EditRunnerForm) {
    const name = data.name.trim()
    const labels = parseLabels(data.labels)
    const nameChanged = !isManaged && name !== runner.name.trim()
    const labelsChanged = labels.join(' ') !== initialLabels.join(' ')
    if (!nameChanged && !labelsChanged) {
      onClose()
      return
    }

    const update: UpdateRunnerRequest = {}
    if (nameChanged) update.name = name
    if (labelsChanged) update.custom_labels = labels

    mutation.mutate(
      { runnerId: runner.id, data: update },
      {
        onSuccess: () => {
          toast.success('Runner updated')
          onClose()
        },
        onError: (error) => {
          toast.error(
            error instanceof Error ? error.message : 'Failed to update runner',
          )
        },
      },
    )
  }

  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) onClose()
      }}
    >
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Edit runner</DialogTitle>
          <DialogDescription>
            Pipelines pick runners by label with <code>runs_on</code>.
          </DialogDescription>
        </DialogHeader>

        <Form {...form}>
          <form onSubmit={form.handleSubmit(onSubmit)} className="space-y-4">
            <FormField
              control={form.control}
              name="name"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>Name</FormLabel>
                  <FormControl>
                    <Input
                      autoFocus={!isManaged}
                      disabled={isManaged}
                      placeholder="Runner name"
                      {...field}
                    />
                  </FormControl>
                  {isManaged ? (
                    <Text variant="caption">
                      Managed runner names are set from the build host.
                    </Text>
                  ) : null}
                  <FormMessage />
                </FormItem>
              )}
            />

            <FormField
              control={form.control}
              name="labels"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>Custom labels</FormLabel>
                  <FormControl>
                    <Input
                      autoFocus={isManaged}
                      placeholder="release, gpu"
                      className="font-mono"
                      {...field}
                    />
                  </FormControl>
                  <Text variant="caption">
                    {detectedLabels.length > 0
                      ? `Detected from the host: ${detectedLabels.join(', ')}.`
                      : 'Detected labels appear after the runner connects.'}
                  </Text>
                  <FormMessage />
                </FormItem>
              )}
            />

            <DialogFooter>
              <Button type="button" variant="outline" onClick={onClose}>
                Cancel
              </Button>
              <Button type="submit" disabled={mutation.isPending}>
                {mutation.isPending ? (
                  <>
                    <Spinner className="size-4" />
                    Saving...
                  </>
                ) : (
                  'Save changes'
                )}
              </Button>
            </DialogFooter>
          </form>
        </Form>
      </DialogContent>
    </Dialog>
  )
}

function RunnersSettingsPage() {
  const navigate = useNavigate({ from: '/settings/runners' })
  const search = useSearch({ from: '/settings/runners' })
  const canWrite = useHasPermission('runners:write')
  const [selectedRunner, setSelectedRunner] = useState<Runner | null>(null)
  const [purgeRunner, setPurgeRunner] = useState<Runner | null>(null)
  const purgeCaches = usePurgeRunnerCaches()

  const page = search.page ?? 1
  const pageSize = search.pageSize ?? 20
  const sort = search.sort ?? 'name'
  const direction = search.direction ?? 'asc'
  const runnersQuery = useRunners({
    q: search.q,
    sort,
    direction,
    limit: pageSize,
    offset: (page - 1) * pageSize,
  })
  const total = runnersQuery.data?.total ?? 0
  const queuedTotal = runnersQuery.data?.queued_total ?? 0
  const unmatchedTotal = runnersQuery.data?.unmatched_queued_total ?? 0
  const onlineTotal = runnersQuery.data?.online_total ?? 0
  const noRunnerOnline =
    runnersQuery.data !== undefined && !search.q && onlineTotal === 0
  const lowDiskRunners = (runnersQuery.data?.runners ?? []).filter(
    (runner) => runner.disk_pressure && runner.status !== 'offline',
  )
  const currentPage = Math.min(page, Math.max(1, Math.ceil(total / pageSize)))
  const visibleRunners = runnersQuery.data?.runners ?? []

  function updateSearch(updates: Partial<RunnersSearch>) {
    void navigate({
      search: (previous) => ({ ...previous, ...updates }),
      replace: true,
    })
  }

  usePageClamp(
    page,
    pageSize,
    runnersQuery.isLoading ? undefined : total,
    (nextPage) => {
      updateSearch({ page: nextPage === 1 ? undefined : nextPage })
    },
  )

  function handleSortChange(nextSort: RunnerSort, next: SortDirection) {
    updateSearch({ sort: nextSort, direction: next, page: undefined })
  }

  return (
    <PageLayout width="wide" fill>
      <PageMeta title="Runners" noindex />
      <PageHeader title="Runners" />

      <DirectRunnerPolicyPanel />

      {lowDiskRunners.length > 0 ? (
        <Alert
          variant={
            lowDiskRunners.some((runner) => runner.disk_pressure === 'critical')
              ? 'destructive'
              : 'default'
          }
        >
          <HugeiconsIcon icon={InformationCircleIcon} />
          <AlertTitle>Runner disk space is low</AlertTitle>
          <AlertDescription>
            {lowDiskRunners.map((runner) => runner.name).join(', ')}{' '}
            {lowDiskRunners.length === 1 ? 'has' : 'have'} less than 20 GB free.
            Clear caches or old workspaces before builds start failing.
          </AlertDescription>
        </Alert>
      ) : null}

      {noRunnerOnline && queuedTotal > 0 && total > 0 ? (
        <Alert>
          <HugeiconsIcon icon={Alert02Icon} className="text-warning!" />
          <AlertTitle>No runner is online</AlertTitle>
          <AlertDescription>
            {queuedTotal} {queuedTotal === 1 ? 'build is' : 'builds are'}{' '}
            waiting and none can start until a runner reconnects. Check that the
            runner service is running on each Mac, then run{' '}
            <code className="font-mono">oore doctor</code> there if it stays
            offline.
          </AlertDescription>
        </Alert>
      ) : runnersQuery.data && total > 0 ? (
        <Text variant="muted">
          {queuedTotal === 0
            ? 'No builds are waiting for a runner.'
            : `${queuedTotal} ${queuedTotal === 1 ? 'build is' : 'builds are'} waiting for a runner.`}
          {unmatchedTotal > 0
            ? ` ${unmatchedTotal} ${unmatchedTotal === 1 ? 'needs' : 'need'} labels no online runner has.`
            : ''}
        </Text>
      ) : null}

      {runnersQuery.error ? (
        <Alert variant="destructive">
          <HugeiconsIcon icon={InformationCircleIcon} />
          <AlertDescription className={splitRowVariants()}>
            <span>Failed to load runners: {runnersQuery.error.message}</span>
            <Button
              variant="outline"
              size="sm"
              onClick={() => void runnersQuery.refetch()}
            >
              Retry
            </Button>
          </AlertDescription>
        </Alert>
      ) : null}

      {!runnersQuery.isLoading &&
      !runnersQuery.error &&
      visibleRunners.length === 0 &&
      !search.q ? (
        <Empty className="border bg-card">
          <EmptyHeader>
            <EmptyMedia variant="icon">
              <HugeiconsIcon icon={ServerStack01Icon} />
            </EmptyMedia>
            <EmptyTitle>No runners connected</EmptyTitle>
            <EmptyDescription>
              {queuedTotal > 0
                ? `${queuedTotal} ${queuedTotal === 1 ? 'build is' : 'builds are'} waiting. `
                : ''}
              Builds start once a Mac with the Runner profile connects. On the
              build Mac, run:
            </EmptyDescription>
          </EmptyHeader>
          <EmptyContent>
            {/* oxlint-disable-next-line layout/max-class-utilities -- install command block */}
            <code className="w-full max-w-md rounded-md border bg-muted px-3 py-2 text-left font-mono text-xs break-all">
              ~/.oore/bin/oore install --profile runner
            </code>
            <Button
              variant="outline"
              nativeButton={false}
              render={
                <a
                  href="https://docs.oore.build/operate/runners/direct"
                  target="_blank"
                  rel="noopener noreferrer"
                />
              }
            >
              Runner setup guide
              <span className="sr-only">(opens in a new tab)</span>
            </Button>
          </EmptyContent>
        </Empty>
      ) : null}

      {!runnersQuery.isLoading &&
      !runnersQuery.error &&
      visibleRunners.length === 0 &&
      search.q ? (
        <Empty className="border bg-card">
          <EmptyHeader>
            <EmptyMedia variant="icon">
              <HugeiconsIcon icon={Search01Icon} />
            </EmptyMedia>
            <EmptyTitle>No matching runners</EmptyTitle>
            <EmptyDescription>
              Try a different search or clear the current query.
            </EmptyDescription>
          </EmptyHeader>
          <EmptyContent>
            <Button
              variant="outline"
              onClick={() => updateSearch({ q: undefined, page: undefined })}
            >
              Clear search
            </Button>
          </EmptyContent>
        </Empty>
      ) : null}

      {!runnersQuery.error && (runnersQuery.isLoading || total > 0) ? (
        <RunnerInventory
          canWrite={canWrite}
          direction={direction}
          isLoading={runnersQuery.isLoading}
          onPageChange={(nextPage) =>
            updateSearch({ page: nextPage > 1 ? nextPage : undefined })
          }
          onPurgeCaches={setPurgeRunner}
          onEdit={setSelectedRunner}
          onSearch={(value) =>
            updateSearch({ q: value.trim() || undefined, page: undefined })
          }
          onSortChange={handleSortChange}
          page={currentPage}
          pageSize={pageSize}
          query={search.q ?? ''}
          runners={visibleRunners}
          sort={sort}
          total={total}
        />
      ) : null}

      {canWrite && selectedRunner ? (
        <EditRunnerDialog
          runner={selectedRunner}
          onClose={() => setSelectedRunner(null)}
        />
      ) : null}

      <ConfirmDialog
        open={canWrite && purgeRunner !== null}
        onOpenChange={(open) => {
          if (!open) setPurgeRunner(null)
        }}
        title={`Purge caches on ${purgeRunner?.name ?? 'runner'}?`}
        description="The runner deletes its pub, Gradle, CocoaPods and Xcode DerivedData caches before its next build. The next build of each project downloads its dependencies again."
        confirmLabel="Purge cache"
        confirmVariant="destructive"
        isPending={purgeCaches.isPending}
        onConfirm={() => {
          if (!purgeRunner) return
          purgeCaches.mutate(purgeRunner.id, {
            onSuccess: () => {
              toast.success('Cache purge requested')
              setPurgeRunner(null)
            },
            onError: (error) => {
              toast.error(
                error instanceof Error
                  ? error.message
                  : 'Failed to request cache purge',
              )
            },
          })
        }}
      />
    </PageLayout>
  )
}
