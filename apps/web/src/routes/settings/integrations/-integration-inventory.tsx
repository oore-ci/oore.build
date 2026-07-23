import { DynamicLucideIcon } from '@/components/ui/dynamic-lucide-icon'
import {
  ArrowLeft as ArrowLeft01Icon,
  ArrowRight as ArrowRight01Icon,
  GitBranch as GitBranchIcon,
  Info as InformationCircleIcon,
  CircleEllipsis as MoreHorizontalCircle01Icon,
  RefreshCw as Refresh01Icon,
  Search as Search01Icon,
} from 'lucide-react'

import RepositoryAvatar from '@/components/repository-avatar'
import { CollectionPagination } from '@/components/collection-controls'
import { CollectionSearchInput } from '@/components/collection-search-input'
import type {
  Integration,
  IntegrationInstallation,
  IntegrationRepository,
} from '@/lib/types'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  DropdownMenu,
  DropdownMenuContent,
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
import { Skeleton } from '@/components/ui/skeleton'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'

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
      className="group flex min-w-0 items-center gap-2 rounded-sm outline-none focus-visible:ring-2 focus-visible:ring-ring [&_span:last-child]:group-hover:underline"
    >
      {content}
    </a>
  ) : (
    <div className="flex min-w-0 items-center gap-2">{content}</div>
  )
}

function RepositoryWebhookAction({
  onSelect,
  repository,
}: {
  onSelect: () => void
  repository: IntegrationRepository
}) {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        render={
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label={`Webhook actions for ${repository.full_name}`}
          />
        }
      >
        <DynamicLucideIcon icon={MoreHorizontalCircle01Icon} />
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-auto">
        <DropdownMenuItem onClick={onSelect}>
          <DynamicLucideIcon icon={Refresh01Icon} />
          Create webhook token
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}

function RepositoryRows({
  canWrite,
  integration,
  onWebhookSelect,
  repositories,
}: {
  canWrite: boolean
  integration: Integration
  onWebhookSelect?: (repository: IntegrationRepository) => void
  repositories: Array<IntegrationRepository>
}) {
  const showWebhookActions =
    integration.provider === 'gitlab' && canWrite && !!onWebhookSelect
  return (
    <>
      <div className="divide-y sm:hidden">
        {repositories.map((repository) => (
          <article key={repository.id} className="space-y-3 py-4">
            <div className="flex items-start justify-between gap-3">
              <RepositoryIdentity
                integration={integration}
                repository={repository}
              />
              {showWebhookActions ? (
                <RepositoryWebhookAction
                  repository={repository}
                  onSelect={() => onWebhookSelect(repository)}
                />
              ) : null}
            </div>
            <div className="flex items-center justify-between gap-3 pl-10">
              <span className="truncate font-mono text-xs text-muted-foreground">
                {repository.default_branch ?? 'Default branch not set'}
              </span>
              <Badge variant={repository.is_private ? 'secondary' : 'outline'}>
                {repository.is_private ? 'Private' : 'Public'}
              </Badge>
            </div>
          </article>
        ))}
      </div>

      <div className="hidden sm:block">
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Repository</TableHead>
              <TableHead>Default branch</TableHead>
              <TableHead className="hidden lg:table-cell">Visibility</TableHead>
              {showWebhookActions ? (
                <TableHead className="w-10">
                  <span className="sr-only">Webhook actions</span>
                </TableHead>
              ) : null}
            </TableRow>
          </TableHeader>
          <TableBody>
            {repositories.map((repository) => (
              <TableRow key={repository.id}>
                <TableCell>
                  <RepositoryIdentity
                    integration={integration}
                    repository={repository}
                  />
                </TableCell>
                <TableCell className="font-mono text-xs text-muted-foreground">
                  {repository.default_branch ?? 'Not set'}
                </TableCell>
                <TableCell className="hidden lg:table-cell">
                  <Badge
                    variant={repository.is_private ? 'secondary' : 'outline'}
                  >
                    {repository.is_private ? 'Private' : 'Public'}
                  </Badge>
                </TableCell>
                {showWebhookActions ? (
                  <TableCell>
                    <RepositoryWebhookAction
                      repository={repository}
                      onSelect={() => onWebhookSelect(repository)}
                    />
                  </TableCell>
                ) : null}
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </div>
    </>
  )
}

function RepositoryPagination({
  onPageChange,
  onPageSizeChange,
  page,
  pageSize,
  total,
}: {
  onPageChange: (page: number) => void
  onPageSizeChange: (pageSize: number) => void
  page: number
  pageSize: number
  total: number
}) {
  if (pageSize !== 10) {
    return (
      <CollectionPagination
        page={page}
        pageSize={pageSize}
        total={total}
        onPageChange={onPageChange}
        onPageSizeChange={onPageSizeChange}
      />
    )
  }

  const totalPages = Math.max(1, Math.ceil(total / pageSize))
  const start = total === 0 ? 0 : (page - 1) * pageSize + 1
  const end = Math.min(page * pageSize, total)

  return (
    <div className="flex flex-col gap-3 border-t pt-4 sm:flex-row sm:items-center sm:justify-between">
      <p className="text-xs text-muted-foreground" aria-live="polite">
        Showing {start}-{end} of {total} repositories
      </p>
      <div className="flex items-center justify-between gap-2">
        <Button
          type="button"
          variant="outline"
          size="sm"
          disabled={page <= 1}
          onClick={() => onPageChange(page - 1)}
        >
          <DynamicLucideIcon icon={ArrowLeft01Icon} aria-hidden />
          Previous
        </Button>
        <span className="min-w-20 text-center text-xs text-muted-foreground">
          Page {page} of {totalPages}
        </span>
        <Button
          type="button"
          variant="outline"
          size="sm"
          disabled={page >= totalPages}
          onClick={() => onPageChange(page + 1)}
        >
          Next
          <DynamicLucideIcon icon={ArrowRight01Icon} aria-hidden />
        </Button>
      </div>
    </div>
  )
}

export function IntegrationRepositoryInventory({
  canWrite,
  error,
  integration,
  isLoading,
  onClearFilters,
  onPageChange,
  onPageSizeChange,
  onRetry,
  onSearch,
  onWebhookTokenRequest,
  page,
  pageSize,
  query,
  repositories,
  repositoryCount,
  total,
}: {
  canWrite: boolean
  error: Error | null
  integration: Integration
  isLoading: boolean
  onClearFilters: () => void
  onPageChange: (page: number) => void
  onPageSizeChange: (pageSize: number) => void
  onRetry: () => void
  onSearch: (query: string) => void
  onWebhookTokenRequest?: (repository: IntegrationRepository) => void
  page: number
  pageSize: number
  query?: string
  repositories: Array<IntegrationRepository>
  repositoryCount: number
  total: number
}) {
  const repositoryKind =
    integration.provider === 'gitlab' ? 'projects' : 'repositories'

  const showControls = isLoading || repositoryCount > 0 || !!query
  const showPagination =
    !isLoading && !error && (total > 20 || page > 1 || pageSize !== 20)

  return (
    <section aria-label="Repositories" className="min-w-0 space-y-4">
      <p className="text-sm text-muted-foreground">
        Repositories discovered from this source are available when an owner or
        admin creates a project.
      </p>

      {showControls ? (
        <div className="flex flex-col gap-3 sm:flex-row sm:items-center">
          <CollectionSearchInput
            initialValue={query ?? ''}
            onSearch={onSearch}
            placeholder={`Search ${repositoryKind}`}
            ariaLabel={`Search ${repositoryKind}`}
          />
        </div>
      ) : null}

      {error ? (
        <Alert variant="destructive">
          <DynamicLucideIcon icon={InformationCircleIcon} size={16} />
          <AlertDescription className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
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

      {!isLoading && !error && repositoryCount === 0 ? (
        <Empty className="bg-card">
          <EmptyHeader>
            <EmptyMedia variant="icon">
              <DynamicLucideIcon icon={GitBranchIcon} />
            </EmptyMedia>
            <EmptyTitle>No synced {repositoryKind}</EmptyTitle>
            <EmptyDescription>
              Sync this source to discover the {repositoryKind} you can use.
            </EmptyDescription>
          </EmptyHeader>
        </Empty>
      ) : null}

      {!isLoading && !error && repositoryCount > 0 && total === 0 ? (
        <Empty className="bg-card">
          <EmptyHeader>
            <EmptyMedia variant="icon">
              <DynamicLucideIcon icon={Search01Icon} />
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
          onWebhookSelect={onWebhookTokenRequest}
          repositories={repositories}
        />
      ) : null}

      {showPagination ? (
        <RepositoryPagination
          page={page}
          pageSize={pageSize}
          total={total}
          onPageChange={onPageChange}
          onPageSizeChange={onPageSizeChange}
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
        <DynamicLucideIcon icon={InformationCircleIcon} size={16} />
        <AlertDescription className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
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
      <Empty className="bg-card">
        <EmptyHeader>
          <EmptyTitle>No {label.toLocaleLowerCase()}</EmptyTitle>
          <EmptyDescription>{emptyDescription}</EmptyDescription>
        </EmptyHeader>
      </Empty>
    )
  }

  return (
    <div className="min-w-0">
      <div className="divide-y sm:hidden">
        {installations.map((installation) => (
          <article key={installation.id} className="space-y-2 py-4">
            <div className="flex items-start justify-between gap-3">
              <h3 className="min-w-0 truncate font-medium">
                {installation.account_name}
              </h3>
              <Badge variant="outline">
                {installation.account_type ?? 'Account'}
              </Badge>
            </div>
            <p className="truncate font-mono text-xs text-muted-foreground">
              {installation.external_id}
            </p>
          </article>
        ))}
      </div>

      <div className="hidden sm:block">
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>{primaryColumnLabel}</TableHead>
              <TableHead>Type</TableHead>
              <TableHead className="hidden lg:table-cell">
                External ID
              </TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {installations.map((installation) => (
              <TableRow key={installation.id}>
                <TableCell className="font-medium">
                  {installation.account_name}
                </TableCell>
                <TableCell>
                  <Badge variant="outline">
                    {installation.account_type ?? 'Account'}
                  </Badge>
                </TableCell>
                <TableCell className="hidden font-mono text-xs text-muted-foreground lg:table-cell">
                  {installation.external_id}
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </div>
    </div>
  )
}
