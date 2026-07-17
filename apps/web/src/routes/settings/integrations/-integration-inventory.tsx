import { useState } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  GitBranchIcon,
  InformationCircleIcon,
  MoreHorizontalCircle01Icon,
  Refresh01Icon,
  Search01Icon,
} from '@hugeicons/core-free-icons'

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
  canWrite,
  onChange,
  pending,
  repository,
}: {
  canWrite: boolean
  onChange: () => void
  pending: boolean
  repository: IntegrationRepository
}) {
  const allowed = repository.allow_direct_macos_runner
  return (
    <div className="flex items-center justify-end gap-2">
      <Badge variant={allowed ? 'success' : 'outline'}>
        {allowed ? 'Allowed' : 'Blocked'}
      </Badge>
      {canWrite ? (
        <Button
          size="sm"
          variant="outline"
          disabled={pending}
          className="min-h-11 sm:min-h-8"
          onClick={onChange}
        >
          {pending ? 'Saving...' : allowed ? 'Block' : 'Allow'}
        </Button>
      ) : null}
    </div>
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
        <HugeiconsIcon icon={MoreHorizontalCircle01Icon} />
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-auto">
        <DropdownMenuItem onClick={onSelect}>
          <HugeiconsIcon icon={Refresh01Icon} />
          Create webhook token
        </DropdownMenuItem>
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
            <RepositoryRunnerPolicy
              canWrite={canWrite}
              onChange={() => onPolicySelect(repository)}
              pending={pendingRepositoryId === repository.id}
              repository={repository}
            />
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
                <TableCell>
                  <RepositoryRunnerPolicy
                    canWrite={canWrite}
                    onChange={() => onPolicySelect(repository)}
                    pending={pendingRepositoryId === repository.id}
                    repository={repository}
                  />
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
        Only allowed repositories can run builds on this Mac. Leaving the rest
        blocked is expected.
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
          <HugeiconsIcon icon={InformationCircleIcon} size={16} />
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
              <HugeiconsIcon icon={GitBranchIcon} />
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
              <HugeiconsIcon icon={Search01Icon} />
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
        <CollectionPagination
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
                ? 'Block new builds?'
                : 'Allow builds on this Mac?'}
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
                : 'Allow builds'}
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
        <HugeiconsIcon icon={InformationCircleIcon} size={16} />
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
