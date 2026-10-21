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
import { InformationCircleIcon, Search01Icon } from '@hugeicons/core-free-icons'

import type { Runner } from '@oore/client/models'
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
import { Alert, AlertDescription } from '@/components/ui/alert'
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

export const Route = createLazyFileRoute('/settings/runners')({
  component: RunnersSettingsPage,
})

const renameRunnerSchema = z.object({
  name: z
    .string()
    .trim()
    .min(1, 'Name is required')
    .max(255, 'Name must be at most 255 characters'),
})

type RenameRunnerForm = z.infer<typeof renameRunnerSchema>

interface RenameRunnerDialogProps {
  runner: Runner
  onClose: () => void
}

function RenameRunnerDialog({ runner, onClose }: RenameRunnerDialogProps) {
  const mutation = useUpdateRunner()
  const form = useForm<RenameRunnerForm>({
    resolver: zodResolver(renameRunnerSchema),
    defaultValues: { name: runner.name },
    mode: 'onBlur',
  })

  const initialName = runner.name
  const isManaged = !runner.registered_by

  function onSubmit(data: RenameRunnerForm) {
    const trimmed = data.name.trim()
    if (trimmed === initialName.trim()) {
      onClose()
      return
    }

    mutation.mutate(
      { runnerId: runner.id, data: { name: trimmed } },
      {
        onSuccess: () => {
          toast.success('Runner renamed')
          onClose()
        },
        onError: (error) => {
          toast.error(
            error instanceof Error ? error.message : 'Failed to rename runner',
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
          <DialogTitle>Rename runner</DialogTitle>
          <DialogDescription>
            Update the display name for this runner.
          </DialogDescription>
        </DialogHeader>

        {isManaged ? (
          <Alert>
            <AlertDescription>
              Managed runner names are set from the build host and cannot be
              changed.
            </AlertDescription>
          </Alert>
        ) : (
          <Form {...form}>
            <form onSubmit={form.handleSubmit(onSubmit)} className="space-y-4">
              <FormField
                control={form.control}
                name="name"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel>Name</FormLabel>
                    <FormControl>
                      <Input autoFocus placeholder="Runner name" {...field} />
                    </FormControl>
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
                    'Save'
                  )}
                </Button>
              </DialogFooter>
            </form>
          </Form>
        )}
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
      <PageHeader
        title="Runners"
        description="Machines that run builds for this instance."
      />

      <DirectRunnerPolicyPanel />

      {runnersQuery.error ? (
        <Alert variant="destructive">
          <HugeiconsIcon icon={InformationCircleIcon} />
          <AlertDescription className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
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
            <EmptyTitle>No runners connected</EmptyTitle>
            <EmptyDescription>
              Runners appear here after they connect to this instance.
            </EmptyDescription>
          </EmptyHeader>
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
          onRename={setSelectedRunner}
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
        <RenameRunnerDialog
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
