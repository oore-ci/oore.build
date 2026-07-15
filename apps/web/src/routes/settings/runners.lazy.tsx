import { useMemo, useState } from 'react'
import {
  createLazyFileRoute,
  useNavigate,
  useSearch,
} from '@tanstack/react-router'
import { useForm } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import * as z from 'zod'
import { toast } from 'sonner'
import { HugeiconsIcon } from '@hugeicons/react'
import { InformationCircleIcon, Search01Icon } from '@hugeicons/core-free-icons'

import type { Runner } from '@/lib/types'
import { useHasPermission } from '@/hooks/use-permissions'
import { useDebouncedCallback } from '@/hooks/use-debounced-callback'
import { usePageClamp } from '@/hooks/use-page-clamp'
import { useRunners, useUpdateRunner } from '@/hooks/use-runners'
import { getRunnerStatusVariant } from '@/lib/status-variants'
import { PageMeta } from '@/lib/seo'
import PageLayout from '@/components/page-layout'
import PageHeader from '@/components/page-header'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
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
import { NativeSelect, NativeSelectOption } from '@/components/ui/native-select'
import { Skeleton } from '@/components/ui/skeleton'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import { Spinner } from '@/components/ui/spinner'
import { Card, CardContent } from '@/components/ui/card'
import RunnerStatusDot from '@/components/runner-status-dot'
import {
  CollectionPagination,
  SortableTableHead,
} from '@/components/collection-controls'
import type { SortDirection } from '@/components/collection-controls'
import type { RunnerSort, RunnersSearch } from './runners'

const EMPTY_RUNNERS: Array<Runner> = []

export const Route = createLazyFileRoute('/settings/runners')({
  component: RunnersSettingsPage,
})

function formatRelativeTime(epochSeconds?: number): string {
  if (!epochSeconds) return 'never'
  const diffSecs = Math.floor(Date.now() / 1000) - epochSeconds
  if (diffSecs < 5) return 'just now'
  if (diffSecs < 60) return `${diffSecs}s ago`
  const mins = Math.floor(diffSecs / 60)
  if (mins < 60) return `${mins}m ago`
  const hrs = Math.floor(mins / 60)
  if (hrs < 24) return `${hrs}h ago`
  const days = Math.floor(hrs / 24)
  return `${days}d ago`
}

function getHeartbeatStaleness(
  epochSeconds?: number,
): 'fresh' | 'stale' | 'none' {
  if (!epochSeconds) return 'none'
  const diffSecs = Math.floor(Date.now() / 1000) - epochSeconds
  if (diffSecs > 60) return 'stale'
  return 'fresh'
}

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
  runner: Runner | null
  open: boolean
  onOpenChange: (open: boolean) => void
}

function RenameRunnerDialog({
  runner,
  open,
  onOpenChange,
}: RenameRunnerDialogProps) {
  const mutation = useUpdateRunner()
  const form = useForm<RenameRunnerForm>({
    resolver: zodResolver(renameRunnerSchema),
    defaultValues: { name: runner?.name ?? '' },
    values: { name: runner?.name ?? '' },
    mode: 'onBlur',
  })

  const initialName = runner?.name ?? ''
  const isEmbedded = !runner?.registered_by

  function handleClose(nextOpen: boolean) {
    if (!nextOpen) {
      form.reset({ name: runner?.name ?? '' })
    }
    onOpenChange(nextOpen)
  }

  function onSubmit(data: RenameRunnerForm) {
    if (!runner) return

    const trimmed = data.name.trim()
    if (trimmed === initialName.trim()) {
      handleClose(false)
      return
    }

    mutation.mutate(
      { runnerId: runner.id, data: { name: trimmed } },
      {
        onSuccess: () => {
          toast.success('Runner renamed')
          handleClose(false)
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
    <Dialog open={open} onOpenChange={handleClose}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Rename runner</DialogTitle>
          <DialogDescription>
            Update the display name for this runner.
          </DialogDescription>
        </DialogHeader>

        {isEmbedded ? (
          <Alert>
            <AlertDescription>
              Embedded runner names are managed by the daemon and cannot be
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
                <Button
                  type="button"
                  variant="outline"
                  onClick={() => handleClose(false)}
                >
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

function RunnerSearch({
  initialValue,
  onSearch,
}: {
  initialValue: string
  onSearch: (value: string) => void
}) {
  const [value, setValue] = useState(initialValue)
  const debouncedSearch = useDebouncedCallback(onSearch, 300)

  return (
    <div className="relative w-full sm:max-w-sm">
      <HugeiconsIcon
        icon={Search01Icon}
        className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground"
        aria-hidden
      />
      <Input
        type="search"
        value={value}
        onChange={(event) => {
          const next = event.target.value
          setValue(next)
          debouncedSearch(next)
        }}
        placeholder="Search runners"
        aria-label="Search runners"
        className="pl-9"
      />
    </div>
  )
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
  const [dialogOpen, setDialogOpen] = useState(false)

  const page = search.page ?? 1
  const pageSize = search.pageSize ?? 20
  const sort = search.sort ?? 'name'
  const direction = search.direction ?? 'asc'
  const runners = runnersQuery.data?.runners ?? EMPTY_RUNNERS
  const onlineCount = useMemo(
    () =>
      runners.filter(
        (runner) => runner.status === 'online' || runner.status === 'busy',
      ).length,
    [runners],
  )
  const offlineCount = useMemo(
    () => runners.filter((runner) => runner.status === 'offline').length,
    [runners],
  )
  const filteredRunners = useMemo(() => {
    const query = search.q?.toLowerCase()
    if (!query) return runners
    return runners.filter((runner) =>
      [
        runner.name,
        runner.id,
        runner.status,
        runner.registered_by ?? 'embedded',
        formatCapabilities(runner.capabilities),
      ].some((value) => value.toLowerCase().includes(query)),
    )
  }, [runners, search.q])
  const sortedRunners = useMemo(
    () =>
      [...filteredRunners].sort((left, right) => {
        const result = compareRunners(left, right, sort)
        return direction === 'asc' ? result : -result
      }),
    [direction, filteredRunners, sort],
  )
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

  function openRename(runner: Runner) {
    setSelectedRunner(runner)
    setDialogOpen(true)
  }

  return (
    <PageLayout width="wide">
      <PageMeta title="Runner Management" noindex />
      <PageHeader
        title="Runners"
        description="Runner health and metadata management. Auto-refreshes every 15s."
      />

      {!canWrite ? (
        <Alert>
          <AlertDescription>
            You have read-only access to runner health and metadata.
          </AlertDescription>
        </Alert>
      ) : null}

      <section className="grid gap-4 md:grid-cols-3">
        <Card>
          <CardContent>
            <p className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
              Total runners
            </p>
            <p className="mt-3 text-2xl font-bold tracking-tight">
              {runners.length}
            </p>
            <p className="mt-1 text-xs text-muted-foreground">
              Embedded and external
            </p>
          </CardContent>
        </Card>
        <Card>
          <CardContent>
            <div className="flex items-center justify-between">
              <p className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                Online runners
              </p>
              {onlineCount > 0 ? (
                <Badge variant="success">{onlineCount}</Badge>
              ) : null}
            </div>
            <p className="mt-3 text-2xl font-bold tracking-tight">
              {onlineCount}
            </p>
            <p className="mt-1 text-xs text-muted-foreground">
              Online or currently busy
            </p>
          </CardContent>
        </Card>
        <Card>
          <CardContent>
            <div className="flex items-center justify-between">
              <p className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                Offline runners
              </p>
              {offlineCount > 0 ? (
                <Badge variant="destructive">{offlineCount}</Badge>
              ) : null}
            </div>
            <p className="mt-3 text-2xl font-bold tracking-tight">
              {offlineCount}
            </p>
            <p className="mt-1 text-xs text-muted-foreground">
              Unreachable or stopped
            </p>
          </CardContent>
        </Card>
      </section>

      <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <RunnerSearch
          key={search.q ?? ''}
          initialValue={search.q ?? ''}
          onSearch={(value) =>
            updateSearch({ q: value.trim() || undefined, page: undefined })
          }
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
          <HugeiconsIcon icon={InformationCircleIcon} size={16} />
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
        <Empty className="bg-card">
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
        <Empty className="bg-card">
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
        <section aria-label="Runner inventory" className="min-w-0">
          <div className="divide-y sm:hidden">
            {runnersQuery.isLoading
              ? Array.from({ length: 4 }, (_, index) => (
                  <div key={index} className="space-y-2 py-4">
                    <Skeleton className="h-5 w-2/3" />
                    <Skeleton className="h-4 w-1/2" />
                  </div>
                ))
              : visibleRunners.map((runner) => {
                  const canRename = canWrite && !!runner.registered_by
                  return (
                    <article key={runner.id} className="space-y-3 py-4">
                      <div className="flex items-start justify-between gap-3">
                        <div className="min-w-0">
                          <h2 className="truncate font-medium">
                            {runner.name}
                          </h2>
                          <p className="truncate font-mono text-xs text-muted-foreground">
                            {runner.id.slice(0, 8)}
                          </p>
                        </div>
                        <div className="flex items-center">
                          <RunnerStatusDot status={runner.status} />
                          <Badge
                            variant={getRunnerStatusVariant(runner.status)}
                          >
                            {runner.status}
                          </Badge>
                        </div>
                      </div>
                      <div className="flex flex-wrap gap-x-3 gap-y-1 text-xs text-muted-foreground">
                        <span>
                          Heartbeat{' '}
                          {formatRelativeTime(runner.last_heartbeat_at)}
                        </span>
                        <span>{runner.registered_by ?? 'Embedded runner'}</span>
                      </div>
                      {canRename ? (
                        <Button
                          variant="outline"
                          size="sm"
                          onClick={() => openRename(runner)}
                        >
                          Rename
                        </Button>
                      ) : null}
                    </article>
                  )
                })}
          </div>

          <div className="hidden sm:block">
            <Table>
              <TableHeader>
                <TableRow>
                  <SortableTableHead
                    sort={sort}
                    sortKey="name"
                    direction={direction}
                    onSortChange={handleSortChange}
                  >
                    Name
                  </SortableTableHead>
                  <SortableTableHead
                    sort={sort}
                    sortKey="status"
                    direction={direction}
                    onSortChange={handleSortChange}
                  >
                    Status
                  </SortableTableHead>
                  <TableHead className="hidden lg:table-cell">
                    Version
                  </TableHead>
                  <SortableTableHead
                    sort={sort}
                    sortKey="last_heartbeat_at"
                    direction={direction}
                    onSortChange={handleSortChange}
                  >
                    Last heartbeat
                  </SortableTableHead>
                  <TableHead className="hidden lg:table-cell">
                    Capabilities
                  </TableHead>
                  <TableHead className="hidden lg:table-cell">
                    Registered by
                  </TableHead>
                  {canWrite ? (
                    <TableHead className="text-right">Action</TableHead>
                  ) : null}
                </TableRow>
              </TableHeader>
              <TableBody>
                {runnersQuery.isLoading
                  ? Array.from({ length: 5 }, (_, index) => (
                      <TableRow key={index}>
                        <TableCell>
                          <Skeleton className="h-8 w-40" />
                        </TableCell>
                        <TableCell>
                          <Skeleton className="h-6 w-20" />
                        </TableCell>
                        <TableCell className="hidden lg:table-cell">
                          <Skeleton className="h-4 w-16" />
                        </TableCell>
                        <TableCell>
                          <Skeleton className="h-4 w-20" />
                        </TableCell>
                        <TableCell className="hidden lg:table-cell">
                          <Skeleton className="h-4 w-36" />
                        </TableCell>
                        <TableCell className="hidden lg:table-cell">
                          <Skeleton className="h-4 w-32" />
                        </TableCell>
                        {canWrite ? (
                          <TableCell>
                            <Skeleton className="ml-auto h-8 w-16" />
                          </TableCell>
                        ) : null}
                      </TableRow>
                    ))
                  : visibleRunners.map((runner) => {
                      const canRename = canWrite && !!runner.registered_by
                      return (
                        <TableRow key={runner.id}>
                          <TableCell>
                            <p className="font-medium">{runner.name}</p>
                            <p className="font-mono text-xs text-muted-foreground">
                              {runner.id.slice(0, 8)}
                            </p>
                          </TableCell>
                          <TableCell>
                            <div className="flex items-center">
                              <RunnerStatusDot status={runner.status} />
                              <Badge
                                variant={getRunnerStatusVariant(runner.status)}
                              >
                                {runner.status}
                              </Badge>
                            </div>
                          </TableCell>
                          <TableCell className="hidden font-mono text-xs text-muted-foreground lg:table-cell">
                            {typeof runner.capabilities.version === 'string'
                              ? runner.capabilities.version
                              : 'Unknown'}
                          </TableCell>
                          <TableCell
                            className={
                              getHeartbeatStaleness(
                                runner.last_heartbeat_at,
                              ) === 'stale' && runner.status !== 'offline'
                                ? 'text-warning'
                                : 'text-muted-foreground'
                            }
                          >
                            {formatRelativeTime(runner.last_heartbeat_at)}
                          </TableCell>
                          <TableCell className="hidden text-xs text-muted-foreground lg:table-cell">
                            {formatCapabilities(runner.capabilities)}
                          </TableCell>
                          <TableCell className="hidden text-sm text-muted-foreground lg:table-cell">
                            {runner.registered_by ?? 'embedded'}
                          </TableCell>
                          {canWrite ? (
                            <TableCell className="text-right">
                              {canRename ? (
                                <Button
                                  variant="outline"
                                  size="sm"
                                  onClick={() => openRename(runner)}
                                >
                                  Rename
                                </Button>
                              ) : (
                                <span className="text-xs text-muted-foreground">
                                  Managed by daemon
                                </span>
                              )}
                            </TableCell>
                          ) : null}
                        </TableRow>
                      )
                    })}
              </TableBody>
            </Table>
          </div>

          {!runnersQuery.isLoading ? (
            <CollectionPagination
              page={currentPage}
              pageSize={pageSize}
              total={total}
              onPageChange={(nextPage) =>
                updateSearch({ page: nextPage > 1 ? nextPage : undefined })
              }
              onPageSizeChange={(nextPageSize) =>
                updateSearch({
                  pageSize:
                    nextPageSize === 20
                      ? undefined
                      : (nextPageSize as 50 | 100),
                  page: undefined,
                })
              }
            />
          ) : null}
        </section>
      ) : null}

      {canWrite ? (
        <RenameRunnerDialog
          open={dialogOpen}
          runner={selectedRunner}
          onOpenChange={(open) => {
            setDialogOpen(() => open)
            if (!open) {
              setSelectedRunner(null)
            }
          }}
        />
      ) : null}
    </PageLayout>
  )
}
