import { useState } from 'react'
import { DynamicLucideIcon } from '@/components/ui/dynamic-lucide-icon'
import {
  ArrowLeft as ArrowLeft01Icon,
  ArrowRight as ArrowRight01Icon,
  X as Cancel01Icon,
  GitBranch as GitBranchIcon,
  Info as InformationCircleIcon,
  CircleEllipsis as MoreHorizontalCircle01Icon,
  RefreshCw as Refresh01Icon,
  Search as Search01Icon,
  Check as Tick02Icon,
} from 'lucide-react'

import RepositoryAvatar from '@/components/repository-avatar'
import { CollectionPagination } from '@/components/collection-controls'
import { CollectionSearchInput } from '@/components/collection-search-input'
import { toast } from '@/lib/toast'
import type {
  Integration,
  IntegrationInstallation,
  IntegrationRepository,
} from '@/lib/types'
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
import { useUpdateRepositoryRunnerPolicy } from '@/hooks/use-integrations'
import type { RepositoryRunnerFilter } from './-integration-inventory-utils'

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

function RepositoryRunnerPolicy({
  repository,
}: {
  repository: IntegrationRepository
}) {
  const allowed = repository.allow_direct_macos_runner
  return (
    <Badge variant={allowed ? 'success' : 'outline'}>
      {allowed ? 'Allowed' : 'Blocked'}
    </Badge>
  )
}

function RepositoryActions({
  onPolicySelect,
  onWebhookSelect,
  pending,
  repository,
}: {
  onPolicySelect: () => void
  onWebhookSelect?: () => void
  pending: boolean
  repository: IntegrationRepository
}) {
  const allowed = repository.allow_direct_macos_runner
  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        render={
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label={`Actions for ${repository.full_name}`}
          />
        }
      >
        <DynamicLucideIcon icon={MoreHorizontalCircle01Icon} />
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-auto">
        <DropdownMenuItem onClick={onPolicySelect} disabled={pending}>
          <DynamicLucideIcon icon={allowed ? Cancel01Icon : Tick02Icon} />
          {pending
            ? 'Saving runner access...'
            : allowed
              ? 'Block new builds'
              : 'Allow builds'}
        </DropdownMenuItem>
        {onWebhookSelect ? (
          <DropdownMenuItem onClick={onWebhookSelect}>
            <DynamicLucideIcon icon={Refresh01Icon} />
            Create webhook token
          </DropdownMenuItem>
        ) : null}
      </DropdownMenuContent>
    </DropdownMenu>
  )
}

function RepositoryRows({
  canWrite,
  integration,
  onPolicySelect,
  onWebhookSelect,
  pendingRepositoryId,
  repositories,
}: {
  canWrite: boolean
  integration: Integration
  onPolicySelect: (repository: IntegrationRepository) => void
  onWebhookSelect?: (repository: IntegrationRepository) => void
  pendingRepositoryId?: string
  repositories: Array<IntegrationRepository>
}) {
  const showActions = canWrite
  const webhookAction =
    integration.provider === 'gitlab' && onWebhookSelect
      ? onWebhookSelect
      : undefined
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
              {showActions ? (
                <RepositoryActions
                  repository={repository}
                  pending={pendingRepositoryId === repository.id}
                  onPolicySelect={() => onPolicySelect(repository)}
                  onWebhookSelect={
                    webhookAction ? () => webhookAction(repository) : undefined
                  }
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
            <div className="flex items-center justify-end">
              <RepositoryRunnerPolicy repository={repository} />
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
              <TableHead className="text-right">Direct runner</TableHead>
              {showActions ? (
                <TableHead className="w-10">
                  <span className="sr-only">Actions</span>
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
                <TableCell>
                  <div className="flex justify-end">
                    <RepositoryRunnerPolicy repository={repository} />
                  </div>
                </TableCell>
                {showActions ? (
                  <TableCell>
                    <RepositoryActions
                      repository={repository}
                      pending={pendingRepositoryId === repository.id}
                      onPolicySelect={() => onPolicySelect(repository)}
                      onWebhookSelect={
                        webhookAction
                          ? () => webhookAction(repository)
                          : undefined
                      }
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
  onRunnerFilterChange,
  onSearch,
  onWebhookTokenRequest,
  page,
  pageSize,
  query,
  repositories,
  repositoryCount,
  runnerFilter,
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
  onRunnerFilterChange: (filter: RepositoryRunnerFilter) => void
  onSearch: (query: string) => void
  onWebhookTokenRequest?: (repository: IntegrationRepository) => void
  page: number
  pageSize: number
  query?: string
  repositories: Array<IntegrationRepository>
  repositoryCount: number
  runnerFilter: RepositoryRunnerFilter
  total: number
}) {
  const policyMutation = useUpdateRepositoryRunnerPolicy(integration.id)
  const [policyTarget, setPolicyTarget] =
    useState<IntegrationRepository | null>(null)
  const repositoryKind =
    integration.provider === 'gitlab' ? 'projects' : 'repositories'

  function updateRunnerPolicy() {
    if (!policyTarget) return
    const target = policyTarget
    const allow = !target.allow_direct_macos_runner
    setPolicyTarget(null)
    policyMutation.mutate(
      { repositoryId: target.id, allow },
      {
        onSuccess: () =>
          toast.success(
            allow
              ? `${target.full_name} can now run builds.`
              : `${target.full_name} is blocked from new builds.`,
          ),
        onError: (mutationError) =>
          toast.error(
            `Could not update ${target.full_name}: ${mutationError.message}`,
          ),
      },
    )
  }

  const showControls =
    isLoading || repositoryCount > 0 || !!query || runnerFilter !== 'all'
  const showPagination =
    !isLoading && !error && (total > 20 || page > 1 || pageSize !== 20)

  return (
    <section aria-label="Repository access" className="min-w-0 space-y-4">
      <p className="text-sm text-muted-foreground">
        Runner access is granted per repository. Blocked is the safe default.
      </p>

      {showControls ? (
        <div className="flex flex-col gap-3 sm:flex-row sm:items-center">
          <CollectionSearchInput
            initialValue={query ?? ''}
            onSearch={onSearch}
            placeholder={`Search ${repositoryKind}`}
            ariaLabel={`Search ${repositoryKind}`}
          />
          <NativeSelect
            className="w-full sm:w-44"
            aria-label="Filter by runner access"
            value={runnerFilter}
            onChange={(event) =>
              onRunnerFilterChange(event.target.value as RepositoryRunnerFilter)
            }
          >
            <NativeSelectOption value="all">
              All runner states
            </NativeSelectOption>
            <NativeSelectOption value="blocked">Blocked</NativeSelectOption>
            <NativeSelectOption value="allowed">Allowed</NativeSelectOption>
          </NativeSelect>
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
            <EmptyDescription>
              Try a different search or runner state.
            </EmptyDescription>
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
          onPolicySelect={setPolicyTarget}
          onWebhookSelect={onWebhookTokenRequest}
          pendingRepositoryId={
            policyMutation.isPending
              ? policyMutation.variables.repositoryId
              : undefined
          }
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

      <AlertDialog
        open={policyTarget !== null}
        onOpenChange={(open) => {
          if (!open) setPolicyTarget(null)
        }}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>
              {policyTarget?.allow_direct_macos_runner
                ? `Block ${policyTarget.full_name}?`
                : `Allow ${policyTarget?.full_name ?? 'this repository'}?`}
            </AlertDialogTitle>
            <AlertDialogDescription>
              {policyTarget?.allow_direct_macos_runner
                ? `Running builds will finish. New builds for projects linked to ${policyTarget.full_name} will wait.`
                : `Build commands from every project linked to ${policyTarget?.full_name ?? 'this repository'} will run with the runner account's macOS permissions.`}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction onClick={updateRunnerPolicy}>
              {policyTarget?.allow_direct_macos_runner
                ? 'Block new builds'
                : 'Allow this repository'}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
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
