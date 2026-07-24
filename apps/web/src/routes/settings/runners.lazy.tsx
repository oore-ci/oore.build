import { useMemo, useState } from 'react'
import {
  createLazyFileRoute,
  useNavigate,
  useSearch,
} from '@tanstack/react-router'
import { useForm } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import * as z from 'zod'
import { toast } from '@/lib/toast'
import {
  Info as InformationCircleIcon,
  Search as Search01Icon,
} from 'lucide-react'

import type { Runner } from '@/lib/types'
import { useHasPermission } from '@/hooks/use-permissions'
import { CollectionSearchInput } from '@/components/collection-search-input'
import { usePageClamp } from '@/hooks/use-page-clamp'
import { useRunners, useUpdateRunner } from '@/hooks/use-runners'
import { PageMeta } from '@/lib/seo'
import PageLayout from '@/components/page-layout'
import PageHeader from '@/components/page-header'
import { DirectRunnerPolicyPanel } from '@/components/settings/direct-runner-policy-panel'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent } from '@/components/ui/card'
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
import { NativeSelect, NativeSelectOption } from '@/components/ui/native-select'
import { Spinner } from '@/components/ui/spinner'
import type { SortDirection } from '@/components/collection-controls'
import type { RunnerSort, RunnersSearch } from './runners'
import { RunnerInventory } from './-runner-inventory'

const EMPTY_RUNNERS: Array<Runner> = []

export const Route = createLazyFileRoute('/settings/runners')({
  component: RunnersSettingsPage,
})

function formatCapabilities(capabilities: Runner['capabilities']): string {
  const entries = Object.entries(capabilities)
  if (entries.length === 0) return 'none'
  return entries
    .slice(0, 3)
    .map(([key, value]) => `${key}:${String(value)}`)
    .join(', ')
}

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

const RUNNER_SORT_OPTIONS: Record<RunnerSort, string> = {
  created_at: 'Registered',
  last_heartbeat_at: 'Last heartbeat',
  name: 'Name',
  status: 'Status',
}

function compareRunners(left: Runner, right: Runner, sort: RunnerSort): number {
  let result = 0
  switch (sort) {
    case 'name':
      result = left.name.localeCompare(right.name)
      break
    case 'status':
      result = left.status.localeCompare(right.status)
      break
    case 'last_heartbeat_at':
      result = (left.last_heartbeat_at ?? 0) - (right.last_heartbeat_at ?? 0)
      break
    case 'created_at':
      result = left.created_at - right.created_at
      break
  }
  return result || left.id.localeCompare(right.id)
}

function RunnersSettingsPage() {
  const runnersQuery = useRunners()
  const navigate = useNavigate({ from: '/settings/runners' })
  const search = useSearch({ from: '/settings/runners' })
  const canWrite = useHasPermission('runners', 'write')
  const [selectedRunner, setSelectedRunner] = useState<Runner | null>(null)

  const page = search.page ?? 1
  const pageSize = search.pageSize ?? 20
  const sort = search.sort ?? 'name'
  const direction = search.direction ?? 'asc'
  const runners = runnersQuery.data?.runners ?? EMPTY_RUNNERS
  const onlineCount = runners.filter(
    (runner) => runner.status === 'online' || runner.status === 'busy',
  ).length
  const offlineCount = runners.filter(
    (runner) => runner.status === 'offline',
  ).length
  const sortedRunners = useMemo(() => {
    const query = search.q?.toLowerCase()
    const matchingRunners = query
      ? runners.filter((runner) =>
          [
            runner.name,
            runner.id,
            runner.status,
            runner.registered_by ?? 'embedded',
            formatCapabilities(runner.capabilities),
          ].some((value) => value.toLowerCase().includes(query)),
        )
      : runners

    return [...matchingRunners].sort((left, right) => {
      const result = compareRunners(left, right, sort)
      return direction === 'asc' ? result : -result
    })
  }, [direction, runners, search.q, sort])
  const total = sortedRunners.length
  const currentPage = Math.min(page, Math.max(1, Math.ceil(total / pageSize)))
  const visibleRunners = sortedRunners.slice(
    (currentPage - 1) * pageSize,
    currentPage * pageSize,
  )

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
    <PageLayout width="wide">
      <PageMeta title="Runners" noindex />
      <PageHeader
        title="Runners"
        description="Manage runner execution policy, health, and metadata. Health refreshes every 15 seconds."
      />

      <DirectRunnerPolicyPanel />

      {!canWrite ? (
        <Alert>
          <AlertDescription>
            You have read-only access to runner health and metadata.
          </AlertDescription>
        </Alert>
      ) : null}

      {!runnersQuery.isLoading && !runnersQuery.error ? (
        <Card size="sm" aria-label="Runner summary">
          <CardContent className="grid gap-4 sm:grid-cols-3">
            <div>
              <p className="text-xs font-medium text-muted-foreground">
                Total runners
              </p>
              <p className="mt-2 text-xl font-semibold tracking-tight">
                {runners.length}
              </p>
              <p className="mt-1 text-xs text-muted-foreground">
                Embedded and external
              </p>
            </div>
            <div>
              <div className="flex items-center justify-between">
                <p className="text-xs font-medium text-muted-foreground">
                  Online runners
                </p>
                {onlineCount > 0 ? (
                  <Badge variant="secondary">{onlineCount}</Badge>
                ) : null}
              </div>
              <p className="mt-2 text-xl font-semibold tracking-tight">
                {onlineCount}
              </p>
              <p className="mt-1 text-xs text-muted-foreground">
                Online or currently busy
              </p>
            </div>
            <div>
              <div className="flex items-center justify-between">
                <p className="text-xs font-medium text-muted-foreground">
                  Offline runners
                </p>
                {offlineCount > 0 ? (
                  <Badge variant="destructive">{offlineCount}</Badge>
                ) : null}
              </div>
              <p className="mt-2 text-xl font-semibold tracking-tight">
                {offlineCount}
              </p>
              <p className="mt-1 text-xs text-muted-foreground">
                Unreachable or stopped
              </p>
            </div>
          </CardContent>
        </Card>
      ) : null}

      <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <CollectionSearchInput
          initialValue={search.q ?? ''}
          onSearch={(value) =>
            updateSearch({ q: value.trim() || undefined, page: undefined })
          }
          placeholder="Search runners"
          ariaLabel="Search runners"
        />
        <div className="flex gap-2 sm:hidden">
          <NativeSelect
            className="min-w-0 flex-1"
            aria-label="Sort runners"
            value={sort}
            onChange={(event) =>
              handleSortChange(event.target.value as RunnerSort, direction)
            }
          >
            {Object.entries(RUNNER_SORT_OPTIONS).map(([value, label]) => (
              <NativeSelectOption key={value} value={value}>
                {label}
              </NativeSelectOption>
            ))}
          </NativeSelect>
          <Button
            variant="outline"
            onClick={() =>
              handleSortChange(sort, direction === 'asc' ? 'desc' : 'asc')
            }
          >
            {direction === 'asc' ? 'Ascending' : 'Descending'}
          </Button>
        </div>
      </div>

      {runnersQuery.error ? (
        <Alert variant="destructive">
          <InformationCircleIcon size={16} />
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
      runners.length === 0 ? (
        <Empty className="border bg-card">
          <EmptyHeader>
            <EmptyTitle>No runners registered</EmptyTitle>
            <EmptyDescription>
              Runners appear here after they connect to this instance.
            </EmptyDescription>
          </EmptyHeader>
        </Empty>
      ) : null}

      {!runnersQuery.isLoading &&
      !runnersQuery.error &&
      runners.length > 0 &&
      total === 0 ? (
        <Empty className="border bg-card">
          <EmptyHeader>
            <EmptyMedia variant="icon">
              <Search01Icon />
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
          onPageSizeChange={(nextPageSize) =>
            updateSearch({
              pageSize:
                nextPageSize === 20 ? undefined : (nextPageSize as 50 | 100),
              page: undefined,
            })
          }
          onRename={setSelectedRunner}
          onSortChange={handleSortChange}
          page={currentPage}
          pageSize={pageSize}
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
    </PageLayout>
  )
}
