import { useMemo, useState } from 'react'
import { Link, createFileRoute, useSearch } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  Delete02Icon,
  InformationCircleIcon,
  Link04Icon,
  MoreHorizontalCircle01Icon,
  Search01Icon,
} from '@hugeicons/core-free-icons'
import { toast } from 'sonner'

import type { Integration } from '@/lib/types'
import { useMountEffect } from '@/hooks/use-mount-effect'
import { useDebouncedCallback } from '@/hooks/use-debounced-callback'
import { usePageClamp } from '@/hooks/use-page-clamp'
import {
  getActiveInstanceOrRedirect,
  requireInstanceRoleOrRedirect,
} from '@/lib/instance-context'
import { useHasPermission } from '@/hooks/use-permissions'
import { useInstancePreferences } from '@/hooks/use-artifact-storage'
import { useDeleteIntegration, useIntegrations } from '@/hooks/use-integrations'
import { getIntegrationStatusVariant } from '@/lib/status-variants'
import { relativeTime } from '@/lib/format-utils'
import { PageMeta } from '@/lib/seo'
import { Alert, AlertDescription } from '@/components/ui/alert'
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { buttonVariants } from '@/components/ui/button-variants'
import {
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from '@/components/ui/card'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from '@/components/ui/empty'
import { Input } from '@/components/ui/input'
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
import {
  CollectionPagination,
  SortableTableHead,
} from '@/components/collection-controls'
import type { SortDirection } from '@/components/collection-controls'
import PageHeader from '@/components/page-header'
import PageLayout from '@/components/page-layout'
import SetupHint from '@/components/setup-hint'

type IntegrationSort = 'name' | 'provider' | 'status' | 'updated_at'

interface IntegrationsSearch {
  direction?: SortDirection
  github?: string
  integration_id?: string
  page?: number
  pageSize?: 20 | 50 | 100
  q?: string
  sort?: IntegrationSort
}

const INTEGRATION_SORTS = new Set<IntegrationSort>([
  'name',
  'provider',
  'status',
  'updated_at',
])

const INTEGRATION_SORT_OPTIONS: Record<IntegrationSort, string> = {
  name: 'Name',
  provider: 'Provider',
  status: 'Status',
  updated_at: 'Recently updated',
}

function parseSearch(search: Record<string, unknown>): IntegrationsSearch {
  const page = Number(search.page)
  const pageSize = Number(search.pageSize)
  const sort = search.sort as IntegrationSort
  const q = typeof search.q === 'string' ? search.q.trim() : ''

  return {
    github: typeof search.github === 'string' ? search.github : undefined,
    integration_id:
      typeof search.integration_id === 'string'
        ? search.integration_id
        : undefined,
    q: q || undefined,
    sort: INTEGRATION_SORTS.has(sort) ? sort : undefined,
    direction: search.direction === 'asc' ? 'asc' : undefined,
    page: Number.isInteger(page) && page > 1 ? page : undefined,
    pageSize: pageSize === 50 || pageSize === 100 ? pageSize : undefined,
  }
}

export const Route = createFileRoute('/settings/integrations/')({
  staticData: { breadcrumbLabel: 'Sources' },
  validateSearch: parseSearch,
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireInstanceRoleOrRedirect(instance.id, ['owner', 'admin', 'developer'])
  },
  component: IntegrationsPage,
})

function SourceSearch({
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
          const nextValue = event.target.value
          setValue(nextValue)
          debouncedSearch(nextValue)
        }}
        placeholder="Search connected sources"
        aria-label="Search connected sources"
        className="pl-9"
      />
    </div>
  )
}

function SourceIdentity({ integration }: { integration: Integration }) {
  return (
    <Link
      to="/settings/integrations/$integrationId"
      params={{ integrationId: integration.id }}
      className="group block min-w-0 rounded-sm outline-none focus-visible:ring-2 focus-visible:ring-ring"
    >
      <span className="block truncate font-medium group-hover:underline">
        {integration.display_name ?? integration.provider}
      </span>
      <span className="block truncate font-mono text-[11px] text-muted-foreground">
        {integration.id.slice(0, 8)}
      </span>
    </Link>
  )
}

function SourceActions({
  integration,
  onDisconnect,
}: {
  integration: Integration
  onDisconnect: () => void
}) {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        render={
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label={`Actions for ${integration.display_name ?? integration.provider}`}
          />
        }
      >
        <HugeiconsIcon icon={MoreHorizontalCircle01Icon} />
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-auto">
        <DropdownMenuItem variant="destructive" onClick={onDisconnect}>
          <HugeiconsIcon icon={Delete02Icon} />
          Disconnect source
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}

function IntegrationsPage() {
  const canWrite = useHasPermission('integrations', 'write')
  const search = useSearch({ from: '/settings/integrations/' })
  const navigate = Route.useNavigate()
  const integrationsQuery = useIntegrations()
  const preferencesQuery = useInstancePreferences({ enabled: canWrite })
  const deleteMutation = useDeleteIntegration()
  const [disconnectTarget, setDisconnectTarget] = useState<Integration | null>(
    null,
  )
  const runtimeMode = preferencesQuery.data?.preferences.runtime_mode
  const remoteEnabled = !canWrite || runtimeMode === 'remote'
  const pageSize = search.pageSize ?? 20
  const sort = search.sort ?? 'updated_at'
  const direction = search.direction ?? 'desc'

  useMountEffect(() => {
    if (search.github === 'success') {
      toast.success('GitHub App connected successfully')
      window.history.replaceState({}, '', '/settings/integrations')
    }
  })

  const filteredIntegrations = useMemo(() => {
    const query = search.q?.toLocaleLowerCase()
    const integrations = (integrationsQuery.data?.integrations ?? []).filter(
      (integration) =>
        query
          ? [
              integration.display_name,
              integration.provider,
              integration.host_url,
              integration.auth_mode,
              integration.status,
            ]
              .filter(Boolean)
              .join(' ')
              .toLocaleLowerCase()
              .includes(query)
          : true,
    )

    return integrations.sort((left, right) => {
      const leftValue =
        sort === 'provider'
          ? left.provider
          : sort === 'status'
            ? left.status
            : sort === 'updated_at'
              ? left.updated_at
              : (left.display_name ?? left.provider).toLocaleLowerCase()
      const rightValue =
        sort === 'provider'
          ? right.provider
          : sort === 'status'
            ? right.status
            : sort === 'updated_at'
              ? right.updated_at
              : (right.display_name ?? right.provider).toLocaleLowerCase()
      const result =
        typeof leftValue === 'number'
          ? leftValue - Number(rightValue)
          : leftValue.localeCompare(String(rightValue))
      return direction === 'asc' ? result : -result
    })
  }, [direction, integrationsQuery.data?.integrations, search.q, sort])

  const total = filteredIntegrations.length
  const requestedPage = search.page ?? 1
  const page = Math.min(requestedPage, Math.max(1, Math.ceil(total / pageSize)))
  const visibleIntegrations = filteredIntegrations.slice(
    (page - 1) * pageSize,
    page * pageSize,
  )

  function updateSearch(updates: Partial<IntegrationsSearch>) {
    void navigate({
      search: (previous) => ({ ...previous, ...updates }),
      replace: true,
    })
  }

  usePageClamp(
    requestedPage,
    pageSize,
    integrationsQuery.isLoading ? undefined : total,
    (nextPage) => {
      updateSearch({ page: nextPage === 1 ? undefined : nextPage })
    },
  )

  function handleSortChange(nextSort: IntegrationSort, next: SortDirection) {
    updateSearch({ sort: nextSort, direction: next, page: undefined })
  }

  function handleDisconnect(integration: Integration) {
    deleteMutation.mutate(integration.id, {
      onSuccess: () => {
        toast.success(
          `Disconnected source: ${integration.display_name ?? integration.provider}`,
        )
        setDisconnectTarget(null)
      },
      onError: (error) => toast.error(`Failed to disconnect: ${error.message}`),
    })
  }

  const hasSearch = !!search.q
  const sourceCount = integrationsQuery.data?.integrations.length ?? 0
  const hasConnectedSources = sourceCount > 0
  const showTrueEmpty =
    remoteEnabled &&
    !integrationsQuery.isLoading &&
    !integrationsQuery.error &&
    total === 0 &&
    !hasSearch
  const showFilteredEmpty =
    remoteEnabled &&
    !integrationsQuery.isLoading &&
    !integrationsQuery.error &&
    total === 0 &&
    hasSearch

  return (
    <PageLayout width="wide">
      <PageMeta title="Sources" noindex />
      <PageHeader
        title="Sources"
        description="Source connections used to discover repositories and trigger builds."
        actions={
          remoteEnabled && canWrite && hasConnectedSources ? (
            <DropdownMenu>
              <DropdownMenuTrigger render={<Button />}>
                <HugeiconsIcon
                  icon={Link04Icon}
                  data-icon="inline-start"
                  aria-hidden
                />
                Connect source
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end">
                <DropdownMenuGroup>
                  <DropdownMenuItem
                    onClick={() =>
                      void navigate({ to: '/settings/integrations/github' })
                    }
                  >
                    GitHub
                  </DropdownMenuItem>
                  <DropdownMenuItem
                    onClick={() =>
                      void navigate({ to: '/settings/integrations/gitlab' })
                    }
                  >
                    GitLab
                  </DropdownMenuItem>
                </DropdownMenuGroup>
              </DropdownMenuContent>
            </DropdownMenu>
          ) : null
        }
      />

      {canWrite && preferencesQuery.isLoading ? (
        <section aria-label="Source access policy" className="space-y-3">
          <Skeleton className="h-5 w-40" />
          <Skeleton className="h-16 w-full" />
        </section>
      ) : canWrite && preferencesQuery.error ? (
        <Alert variant="destructive">
          <HugeiconsIcon icon={InformationCircleIcon} size={16} />
          <AlertDescription className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
            <span>
              Failed to load access policy: {preferencesQuery.error.message}
            </span>
            <Button
              variant="outline"
              size="sm"
              onClick={() => void preferencesQuery.refetch()}
            >
              Retry
            </Button>
          </AlertDescription>
        </Alert>
      ) : !remoteEnabled ? (
        <section className="space-y-4" aria-labelledby="external-access-title">
          <div>
            <h2
              id="external-access-title"
              className="text-sm font-medium uppercase tracking-wider text-muted-foreground"
            >
              External access required
            </h2>
            <p className="mt-2 text-sm text-muted-foreground">
              GitHub and GitLab connections require Remote mode. In Local Only
              mode, choose a repository path during project creation.
            </p>
          </div>
          <SetupHint
            title="Local only path"
            items={[
              'Create a project from a repository path available on the runner host.',
              'Switch to Remote mode only when browser users or external webhooks need to reach the backend.',
            ]}
          />
          <div className="flex flex-wrap gap-2">
            <Button
              variant="outline"
              render={<Link to="/settings/preferences" />}
              nativeButton={false}
            >
              Open preferences
            </Button>
            <Button render={<Link to="/projects" />} nativeButton={false}>
              Go to projects
            </Button>
          </div>
        </section>
      ) : null}

      {remoteEnabled &&
      (integrationsQuery.isLoading ||
        integrationsQuery.error ||
        hasConnectedSources ||
        hasSearch ||
        !canWrite) ? (
        <section
          aria-label="Connected sources"
          className="flex min-w-0 flex-col gap-4"
        >
          {integrationsQuery.isLoading || hasConnectedSources || hasSearch ? (
            <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
              <SourceSearch
                key={search.q ?? ''}
                initialValue={search.q ?? ''}
                onSearch={(value) =>
                  updateSearch({
                    q: value.trim() || undefined,
                    page: undefined,
                  })
                }
              />
              <NativeSelect
                className="w-full sm:hidden"
                aria-label="Sort connected sources"
                value={sort}
                onChange={(event) =>
                  handleSortChange(
                    event.target.value as IntegrationSort,
                    direction,
                  )
                }
              >
                {Object.entries(INTEGRATION_SORT_OPTIONS).map(
                  ([value, label]) => (
                    <NativeSelectOption key={value} value={value}>
                      {label}
                    </NativeSelectOption>
                  ),
                )}
              </NativeSelect>
            </div>
          ) : null}

          {integrationsQuery.error ? (
            <Alert variant="destructive">
              <HugeiconsIcon icon={InformationCircleIcon} size={16} />
              <AlertDescription className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
                <span>
                  Failed to load sources: {integrationsQuery.error.message}
                </span>
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => void integrationsQuery.refetch()}
                >
                  Retry
                </Button>
              </AlertDescription>
            </Alert>
          ) : null}

          {showTrueEmpty ? (
            <Empty className="bg-card">
              <EmptyHeader>
                <EmptyMedia variant="icon">
                  <HugeiconsIcon icon={Link04Icon} />
                </EmptyMedia>
                <EmptyTitle>No connected sources</EmptyTitle>
                <EmptyDescription>
                  {canWrite
                    ? 'Choose GitHub or GitLab below to discover repositories.'
                    : 'An owner or admin can connect the first source.'}
                </EmptyDescription>
              </EmptyHeader>
            </Empty>
          ) : null}

          {showFilteredEmpty ? (
            <Empty className="bg-card">
              <EmptyHeader>
                <EmptyMedia variant="icon">
                  <HugeiconsIcon icon={Search01Icon} />
                </EmptyMedia>
                <EmptyTitle>No matching sources</EmptyTitle>
                <EmptyDescription>
                  Try a different search or clear the current query.
                </EmptyDescription>
              </EmptyHeader>
              <EmptyContent>
                <Button
                  variant="outline"
                  onClick={() =>
                    updateSearch({ q: undefined, page: undefined })
                  }
                >
                  Clear search
                </Button>
              </EmptyContent>
            </Empty>
          ) : null}

          {!integrationsQuery.error &&
          (integrationsQuery.isLoading || total > 0) ? (
            <div aria-label="Connected source inventory" className="min-w-0">
              <div className="divide-y sm:hidden">
                {integrationsQuery.isLoading
                  ? Array.from({ length: 3 }, (_, index) => (
                      <div key={index} className="space-y-2 py-4">
                        <Skeleton className="h-5 w-2/3" />
                        <Skeleton className="h-4 w-1/2" />
                      </div>
                    ))
                  : visibleIntegrations.map((integration) => (
                      <article key={integration.id} className="space-y-3 py-4">
                        <div className="flex items-start justify-between gap-3">
                          <SourceIdentity integration={integration} />
                          {canWrite ? (
                            <SourceActions
                              integration={integration}
                              onDisconnect={() =>
                                setDisconnectTarget(integration)
                              }
                            />
                          ) : null}
                        </div>
                        <div className="flex flex-wrap items-center gap-2">
                          <Badge variant="outline">
                            {integration.provider}
                          </Badge>
                          <Badge
                            variant={getIntegrationStatusVariant(
                              integration.status,
                            )}
                          >
                            {integration.status}
                          </Badge>
                          <span className="text-xs text-muted-foreground">
                            Updated {relativeTime(integration.updated_at)}
                          </span>
                        </div>
                      </article>
                    ))}
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
                        Source
                      </SortableTableHead>
                      <SortableTableHead
                        sort={sort}
                        sortKey="provider"
                        direction={direction}
                        onSortChange={handleSortChange}
                      >
                        Provider
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
                        Authentication
                      </TableHead>
                      <TableHead className="hidden lg:table-cell">
                        Host
                      </TableHead>
                      <SortableTableHead
                        className="hidden lg:table-cell"
                        sort={sort}
                        sortKey="updated_at"
                        direction={direction}
                        onSortChange={handleSortChange}
                      >
                        Updated
                      </SortableTableHead>
                      {canWrite ? (
                        <TableHead className="text-right">
                          <span className="sr-only">Actions</span>
                        </TableHead>
                      ) : null}
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {integrationsQuery.isLoading
                      ? Array.from({ length: 4 }, (_, index) => (
                          <TableRow key={index}>
                            <TableCell>
                              <Skeleton className="h-8 w-40" />
                            </TableCell>
                            <TableCell>
                              <Skeleton className="h-6 w-16" />
                            </TableCell>
                            <TableCell>
                              <Skeleton className="h-6 w-16" />
                            </TableCell>
                            <TableCell className="hidden lg:table-cell">
                              <Skeleton className="h-4 w-24" />
                            </TableCell>
                            <TableCell className="hidden lg:table-cell">
                              <Skeleton className="h-4 w-36" />
                            </TableCell>
                            <TableCell className="hidden lg:table-cell">
                              <Skeleton className="h-4 w-20" />
                            </TableCell>
                            {canWrite ? (
                              <TableCell>
                                <Skeleton className="ml-auto h-8 w-8" />
                              </TableCell>
                            ) : null}
                          </TableRow>
                        ))
                      : visibleIntegrations.map((integration) => (
                          <TableRow key={integration.id}>
                            <TableCell>
                              <SourceIdentity integration={integration} />
                            </TableCell>
                            <TableCell>
                              <Badge variant="outline">
                                {integration.provider}
                              </Badge>
                            </TableCell>
                            <TableCell>
                              <Badge
                                variant={getIntegrationStatusVariant(
                                  integration.status,
                                )}
                              >
                                {integration.status}
                              </Badge>
                            </TableCell>
                            <TableCell className="hidden font-mono text-xs text-muted-foreground lg:table-cell">
                              {integration.auth_mode}
                            </TableCell>
                            <TableCell className="hidden max-w-[24ch] truncate text-xs text-muted-foreground lg:table-cell">
                              {integration.host_url}
                            </TableCell>
                            <TableCell className="hidden text-xs text-muted-foreground lg:table-cell">
                              {relativeTime(integration.updated_at)}
                            </TableCell>
                            {canWrite ? (
                              <TableCell className="text-right">
                                <SourceActions
                                  integration={integration}
                                  onDisconnect={() =>
                                    setDisconnectTarget(integration)
                                  }
                                />
                              </TableCell>
                            ) : null}
                          </TableRow>
                        ))}
                  </TableBody>
                </Table>
              </div>

              {!integrationsQuery.isLoading ? (
                <CollectionPagination
                  page={page}
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
            </div>
          ) : null}
        </section>
      ) : null}

      {remoteEnabled &&
      canWrite &&
      !integrationsQuery.isLoading &&
      !integrationsQuery.error &&
      !hasConnectedSources ? (
        <section
          className="flex flex-col gap-4"
          aria-labelledby="connect-source-title"
        >
          <div>
            <h2
              id="connect-source-title"
              className="text-sm font-medium uppercase tracking-wider text-muted-foreground"
            >
              Connect a source
            </h2>
            <p className="mt-1 text-sm text-muted-foreground">
              Choose a provider to start discovering repositories.
            </p>
          </div>

          <div className="grid items-stretch gap-4 md:grid-cols-2">
            <Card size="sm">
              <CardHeader>
                <CardTitle>GitHub</CardTitle>
                <CardDescription>
                  Create and install a GitHub App for repository discovery and
                  webhook events.
                </CardDescription>
              </CardHeader>
              <CardContent className="flex-1">
                <div className="flex h-full flex-col gap-2 text-xs text-muted-foreground">
                  <p className="font-medium uppercase tracking-wider text-foreground">
                    Requested access
                  </p>
                  <ul className="flex list-disc flex-col gap-1 pl-4 leading-relaxed">
                    <li>Repository contents and metadata read access.</li>
                    <li>
                      Pull request read plus statuses/checks write access.
                    </li>
                    <li>Push and pull request webhook events.</li>
                  </ul>
                </div>
              </CardContent>
              <CardFooter className="mt-auto">
                <Link
                  to="/settings/integrations/github"
                  className={buttonVariants({
                    size: 'sm',
                    className: 'w-full sm:w-auto',
                  })}
                >
                  <HugeiconsIcon
                    icon={Link04Icon}
                    data-icon="inline-start"
                    aria-hidden
                  />
                  Connect GitHub
                </Link>
              </CardFooter>
            </Card>

            <Card size="sm">
              <CardHeader>
                <CardTitle>GitLab</CardTitle>
                <CardDescription>
                  Connect GitLab.com or a self-managed host with a personal
                  access token or OAuth application.
                </CardDescription>
              </CardHeader>
              <CardContent className="flex-1">
                <div className="flex h-full flex-col gap-2 text-xs text-muted-foreground">
                  <p className="font-medium uppercase tracking-wider text-foreground">
                    Token scopes
                  </p>
                  <ul className="flex list-disc flex-col gap-1 pl-4 leading-relaxed">
                    <li>
                      Use <code>read_user</code>, <code>read_api</code>, and{' '}
                      <code>read_repository</code>.
                    </li>
                    <li>
                      Avoid full <code>api</code> unless a write feature needs
                      it.
                    </li>
                  </ul>
                </div>
              </CardContent>
              <CardFooter className="mt-auto">
                <Link
                  to="/settings/integrations/gitlab"
                  className={buttonVariants({
                    size: 'sm',
                    className: 'w-full sm:w-auto',
                  })}
                >
                  <HugeiconsIcon
                    icon={Link04Icon}
                    data-icon="inline-start"
                    aria-hidden
                  />
                  Connect GitLab
                </Link>
              </CardFooter>
            </Card>
          </div>
        </section>
      ) : null}

      {remoteEnabled && !canWrite ? (
        <Alert>
          <HugeiconsIcon icon={InformationCircleIcon} size={16} />
          <AlertDescription>
            You have read-only access to connected sources. An owner or admin
            can add, reconnect, or disconnect providers.
          </AlertDescription>
        </Alert>
      ) : null}

      <AlertDialog
        open={disconnectTarget !== null}
        onOpenChange={(open) => {
          if (!open) setDisconnectTarget(null)
        }}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Disconnect source?</AlertDialogTitle>
            <AlertDialogDescription>
              This removes credentials, installations, repository links, and
              webhook behavior for{' '}
              {disconnectTarget?.display_name ??
                disconnectTarget?.provider ??
                'this source'}
              .
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              onClick={() => {
                if (disconnectTarget) handleDisconnect(disconnectTarget)
              }}
              disabled={deleteMutation.isPending}
            >
              {deleteMutation.isPending ? 'Disconnecting...' : 'Disconnect'}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </PageLayout>
  )
}
