import { useMemo } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  GitBranchIcon,
  InformationCircleIcon,
  Refresh01Icon,
  Search01Icon,
} from '@hugeicons/core-free-icons'

import RepositoryAvatar from '@/components/repository-avatar'
import {
  DataTable,
  useDataTable,
  type DataTableColumnDef,
  DataTableRowActionsTrigger,
} from '@/components/data-table'
import type {
  Integration,
  IntegrationInstallation,
  IntegrationRepository,
} from '@oore/client/models'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
} from '@/components/ui/dropdown-menu'
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from '@/components/ui/empty'
import { Skeleton } from '@/components/ui/skeleton'
import { Row } from '@/components/layout/row'
import { Text } from '@/components/layout/text'
import { focusRingVariants, rowVariants, splitRowVariants } from '@/lib/recipes'
import { cn } from '@/lib/utils'

function repositoryUrl(
  integration: Integration,
  repository: IntegrationRepository,
): string | null {
  if (integration.provider === 'local_git') return null
  return `${integration.host_url.replace(/\/$/, '')}/${repository.full_name}`
}

function RepositoryIdentity({
  integration,
  repository,
}: {
  integration: Integration
  repository: IntegrationRepository
}) {
  const content = (
    <>
      <RepositoryAvatar
        fullName={repository.full_name}
        avatarUrl={repository.avatar_url}
        repositoryId={repository.id}
        provider={integration.provider}
      />
      <span className="min-w-0 truncate font-medium">
        {repository.full_name}
      </span>
    </>
  )
  const url = repositoryUrl(integration, repository)

  return url ? (
    <a
      href={url}
      target="_blank"
      rel="noreferrer"
      className={cn(
        rowVariants({ gap: 2 }),
        focusRingVariants(),
        'group min-w-0 rounded-md [&_span:last-child]:group-hover:underline',
      )}
    >
      {content}
    </a>
  ) : (
    <Row gap={2} className="min-w-0">
      {content}
    </Row>
  )
}

function RepositoryWebhookAction({
  name,
  onSelect,
}: {
  name: string
  onSelect: () => void
}) {
  return (
    <DropdownMenu>
      <DataTableRowActionsTrigger label={`Actions for ${name}`} />
      <DropdownMenuContent align="end" className="w-44">
        <DropdownMenuGroup>
          <DropdownMenuLabel>Actions</DropdownMenuLabel>
          <DropdownMenuItem onClick={onSelect}>
            <HugeiconsIcon icon={Refresh01Icon} />
            Create webhook token
          </DropdownMenuItem>
        </DropdownMenuGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}

function RepositoryRows({
  canWrite,
  integration,
  onPageChange,
  onSearch,
  onWebhookSelect,
  page,
  pageSize,
  query,
  repositories,
  total,
}: {
  canWrite: boolean
  integration: Integration
  onPageChange: (page: number) => void
  onSearch: (query: string) => void
  onWebhookSelect?: (repository: IntegrationRepository) => void
  page: number
  pageSize: number
  query: string
  repositories: Array<IntegrationRepository>
  total: number
}) {
  const showWebhookActions =
    integration.provider === 'gitlab' && canWrite && !!onWebhookSelect
  const columns = useMemo<
    Array<DataTableColumnDef<IntegrationRepository>>
  >(() => {
    const result: Array<DataTableColumnDef<IntegrationRepository>> = [
      {
        accessorKey: 'full_name',
        header: 'Repository',
        cell: ({ row }) => (
          <RepositoryIdentity
            integration={integration}
            repository={row.original}
          />
        ),
        enableSorting: false,
      },
      {
        accessorKey: 'default_branch',
        header: 'Default branch',
        cell: ({ row }) => row.original.default_branch ?? 'Not set',
        enableSorting: false,
      },
      {
        accessorKey: 'is_private',
        header: 'Visibility',
        cell: ({ row }) => (
          <Badge variant={row.original.is_private ? 'secondary' : 'outline'}>
            {row.original.is_private ? 'Private' : 'Public'}
          </Badge>
        ),
        enableSorting: false,
      },
    ]
    if (showWebhookActions) {
      result.push({
        id: 'actions',
        header: () => <span className="sr-only">Webhook actions</span>,
        cell: ({ row }) => (
          <RepositoryWebhookAction
            name={row.original.full_name}
            onSelect={() => onWebhookSelect(row.original)}
          />
        ),
        enableHiding: false,
        enableSorting: false,
      })
    }
    return result
  }, [integration, onWebhookSelect, showWebhookActions])
  const table = useDataTable({
    columns,
    data: repositories,
    getRowId: (repository) => repository.id,
  })
  return (
    <DataTable
      table={table}
      search={{
        value: query,
        onChange: onSearch,
        placeholder: 'Search repositories',
      }}
      pagination={{ onPageChange, page, pageSize, total }}
    />
  )
}

function InstallationTable({
  installations,
  primaryColumnLabel,
}: {
  installations: Array<IntegrationInstallation>
  primaryColumnLabel: string
}) {
  const columns = useMemo<Array<DataTableColumnDef<IntegrationInstallation>>>(
    () => [
      {
        accessorKey: 'account_name',
        header: primaryColumnLabel,
        enableSorting: false,
      },
      {
        accessorKey: 'account_type',
        header: 'Type',
        cell: ({ row }) => (
          <Badge variant="outline">
            {row.original.account_type ?? 'Account'}
          </Badge>
        ),
        enableSorting: false,
      },
      {
        accessorKey: 'external_id',
        header: 'External ID',
        enableSorting: false,
      },
    ],
    [primaryColumnLabel],
  )
  const table = useDataTable({
    columns,
    data: installations,
    getRowId: (installation) => installation.id,
  })
  return <DataTable table={table} />
}

export function IntegrationRepositoryInventory({
  canWrite,
  error,
  integration,
  isLoading,
  onClearFilters,
  onPageChange,
  onRetry,
  onSearch,
  onWebhookTokenRequest,
  page,
  pageSize,
  query,
  repositories,
  total,
}: {
  canWrite: boolean
  error: Error | null
  integration: Integration
  isLoading: boolean
  onClearFilters: () => void
  onPageChange: (page: number) => void
  onRetry: () => void
  onSearch: (query: string) => void
  onWebhookTokenRequest?: (repository: IntegrationRepository) => void
  page: number
  pageSize: number
  query?: string
  repositories: Array<IntegrationRepository>
  total: number
}) {
  const repositoryKind = 'repositories'

  return (
    <section aria-label="Repositories" className="min-w-0 space-y-4">
      <Text variant="muted">
        Repositories discovered from this source are available when an owner or
        admin creates a project.
      </Text>

      {error ? (
        <Alert variant="destructive">
          <HugeiconsIcon icon={InformationCircleIcon} size={16} />
          <AlertDescription className={splitRowVariants()}>
            <span>
              Could not load {repositoryKind}: {error.message}
            </span>
            <Button variant="outline" size="sm" onClick={onRetry}>
              Retry
            </Button>
          </AlertDescription>
        </Alert>
      ) : null}

      {isLoading ? (
        <div className="space-y-3" aria-label={`Loading ${repositoryKind}`}>
          {Array.from({ length: 5 }, (_, index) => (
            <Skeleton key={index} className="h-12 w-full" />
          ))}
        </div>
      ) : null}

      {!isLoading && !error && !query && total === 0 ? (
        <Empty className="border bg-card">
          <EmptyHeader>
            <EmptyMedia variant="icon">
              <HugeiconsIcon icon={GitBranchIcon} />
            </EmptyMedia>
            <EmptyTitle>No synced {repositoryKind}</EmptyTitle>
            <EmptyDescription>
              Sync this source to discover the {repositoryKind} you can use.
            </EmptyDescription>
          </EmptyHeader>
        </Empty>
      ) : null}

      {!isLoading && !error && query && total === 0 ? (
        <Empty className="border bg-card">
          <EmptyHeader>
            <EmptyMedia variant="icon">
              <HugeiconsIcon icon={Search01Icon} />
            </EmptyMedia>
            <EmptyTitle>No matching {repositoryKind}</EmptyTitle>
            <EmptyDescription>Try a different search.</EmptyDescription>
          </EmptyHeader>
          <EmptyContent>
            <Button variant="outline" onClick={onClearFilters}>
              Clear filters
            </Button>
          </EmptyContent>
        </Empty>
      ) : null}

      {!isLoading && !error && repositories.length > 0 ? (
        <RepositoryRows
          canWrite={canWrite}
          integration={integration}
          onPageChange={onPageChange}
          onSearch={onSearch}
          onWebhookSelect={onWebhookTokenRequest}
          page={page}
          pageSize={pageSize}
          query={query ?? ''}
          repositories={repositories}
          total={total}
        />
      ) : null}
    </section>
  )
}

export function IntegrationAccountsInventory({
  emptyDescription,
  error,
  installations,
  isLoading,
  label,
  onRetry,
  primaryColumnLabel = 'Account',
}: {
  emptyDescription: string
  error: Error | null
  installations: Array<IntegrationInstallation>
  isLoading: boolean
  label: string
  onRetry: () => void
  primaryColumnLabel?: string
}) {
  if (error) {
    return (
      <Alert variant="destructive">
        <HugeiconsIcon icon={InformationCircleIcon} size={16} />
        <AlertDescription className={splitRowVariants()}>
          <span>
            Could not load {label.toLocaleLowerCase()}: {error.message}
          </span>
          <Button variant="outline" size="sm" onClick={onRetry}>
            Retry
          </Button>
        </AlertDescription>
      </Alert>
    )
  }

  if (isLoading) {
    return (
      <div
        className="space-y-3"
        aria-label={`Loading ${label.toLocaleLowerCase()}`}
      >
        {Array.from({ length: 3 }, (_, index) => (
          <Skeleton key={index} className="h-12 w-full" />
        ))}
      </div>
    )
  }

  if (installations.length === 0) {
    return (
      <Empty className="border bg-card">
        <EmptyHeader>
          <EmptyTitle>No {label.toLocaleLowerCase()}</EmptyTitle>
          <EmptyDescription>{emptyDescription}</EmptyDescription>
        </EmptyHeader>
      </Empty>
    )
  }

  return (
    <InstallationTable
      installations={installations}
      primaryColumnLabel={primaryColumnLabel}
    />
  )
}
