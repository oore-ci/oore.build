import { useMemo, useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { createFileRoute, useSearch } from '@tanstack/react-router'
import { DynamicLucideIcon } from '@/components/ui/dynamic-lucide-icon'
import {
  Info as InformationCircleIcon,
} from 'lucide-react'

import { toast } from '@/lib/toast'
import { useMountEffect } from '@/hooks/use-mount-effect'
import { usePageClamp } from '@/hooks/use-page-clamp'
import { useIsBelowBreakpoint } from '@/hooks/use-mobile'
import {
  getActiveInstanceOrRedirect,
  requireInstanceRoleOrRedirect,
} from '@/lib/instance-context'
import { useHasPermission } from '@/hooks/use-permissions'
import { useBreadcrumbLabel } from '@/hooks/use-breadcrumb-label'
import {
  useDeleteIntegration,
  useGitLabAuthorize,
  useInstallations,
  useIntegration,
  useIntegrationRepos,
  useSyncInstallations,
} from '@/hooks/use-integrations'
import { getIntegrationStatusVariant } from '@/lib/status-variants'
import { useExternalAccessNetworkSettings } from '@/hooks/use-artifact-storage'
import { listProjects } from '@/lib/api'
import { gitLabPublicEndpoints } from '@/lib/gitlab-url'
import { resolveInstanceApiBaseUrl } from '@/lib/instance-url'
import { PageMeta } from '@/lib/seo'
import { useAuthStore } from '@/stores/auth-store'
import { useActiveInstance } from '@/stores/instance-store'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import PageHeader from '@/components/page-header'
import PageLayout from '@/components/page-layout'
import { Skeleton } from '@/components/ui/skeleton'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import type { IntegrationRepository } from '@/lib/types'
import {
  IntegrationAccountsInventory,
  IntegrationRepositoryInventory,
} from './-integration-inventory'
import {
  filterIntegrationRepositories,
  paginateIntegrationRepositories,
} from './-integration-inventory-utils'
import type { RepositoryRunnerFilter } from './-integration-inventory-utils'
import { GitLabWebhookTokenDialogs } from './-gitlab-webhook-tokens'
import { IntegrationConnectionDetails } from './-integration-connection-details'
import { IntegrationDisconnectDialog } from './-integration-disconnect-dialog'
import { loadAffectedProjects } from './-integration-disconnect-impact'
import { IntegrationHeaderActions } from './-integration-header-actions'

type IntegrationDetailTab = 'repositories' | 'accounts' | 'connection'

const EMPTY_REPOSITORIES: Array<IntegrationRepository> = []

interface IntegrationDetailSearch {
  gitlab?: string
  installed?: string
  page?: number
  pageSize?: 10 | 20 | 50 | 100
  q?: string
  runner?: Exclude<RepositoryRunnerFilter, 'all'>
  tab?: Exclude<IntegrationDetailTab, 'repositories'>
}

function parseSearch(search: Record<string, unknown>): IntegrationDetailSearch {
  const page = Number(search.page)
  const pageSize = Number(search.pageSize)
  const q = typeof search.q === 'string' ? search.q.trim() : ''
  const tab = search.tab
  const runner = search.runner

  return {
    installed:
      typeof search.installed === 'string' ? search.installed : undefined,
    gitlab: typeof search.gitlab === 'string' ? search.gitlab : undefined,
    q: q || undefined,
    runner: runner === 'allowed' || runner === 'blocked' ? runner : undefined,
    tab: tab === 'accounts' || tab === 'connection' ? tab : undefined,
    page: Number.isInteger(page) && page > 1 ? page : undefined,
    pageSize:
      pageSize === 10 || pageSize === 20 || pageSize === 50 || pageSize === 100
        ? pageSize
        : undefined,
  }
}

export const Route = createFileRoute('/settings/integrations/$integrationId')({
  staticData: {
    breadcrumbLabel: 'Details',
    breadcrumbParent: { label: 'Sources', to: '/settings/integrations' },
  },
  validateSearch: parseSearch,
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireInstanceRoleOrRedirect(instance.id, ['owner', 'admin', 'developer'])
  },
  component: IntegrationDetailPage,
})

function useIntegrationDetailPageState(
  canWrite: boolean,
  disconnectOpen: boolean,
) {
  const { integrationId } = Route.useParams()
  const search = useSearch({ from: '/settings/integrations/$integrationId' })
  const navigate = Route.useNavigate()

  const detailQuery = useIntegration(integrationId)
  const installationsQuery = useInstallations(integrationId)
  const repositoriesQuery = useIntegrationRepos(integrationId)
  const isCompact = useIsBelowBreakpoint(640)
  const instance = useActiveInstance()
  const token = useAuthStore((state) => state.token)
  const baseUrl = resolveInstanceApiBaseUrl(instance)
  const networkSettingsQuery = useExternalAccessNetworkSettings({
    enabled: canWrite,
  })
  const syncMutation = useSyncInstallations()
  const deleteMutation = useDeleteIntegration()
  const gitlabAuthorizeMutation = useGitLabAuthorize()

  const label =
    detailQuery.data?.integration.display_name ??
    detailQuery.data?.integration.provider ??
    'Source Details'
  const repositories =
    repositoriesQuery.data?.repositories ?? EMPTY_REPOSITORIES
  const repositoryIds = useMemo(
    () => repositories.map((repository) => repository.id).sort(),
    [repositories],
  )
  const disconnectPrerequisiteError =
    disconnectOpen && (!baseUrl || !token)
      ? new Error('Active instance authentication is unavailable.')
      : null
  const disconnectImpactQuery = useQuery({
    queryKey: [
      instance?.id ?? '__none__',
      'integration-disconnect-impact',
      integrationId,
      repositoryIds,
    ],
    queryFn: ({ signal }) =>
      loadAffectedProjects(new Set(repositoryIds), (offset, limit) =>
        listProjects(
          baseUrl!,
          token!,
          { limit, offset, sort: 'name', direction: 'asc' },
          { signal },
        ),
      ),
    enabled:
      disconnectOpen &&
      canWrite &&
      !!baseUrl &&
      !!token &&
      !repositoriesQuery.isLoading &&
      !repositoriesQuery.error,
  })
  const runnerFilter: RepositoryRunnerFilter = search.runner ?? 'all'
  const defaultPageSize = isCompact ? 10 : 20
  const pageSize = search.pageSize ?? defaultPageSize
  const filteredRepositories = useMemo(
    () => filterIntegrationRepositories(repositories, search.q, runnerFilter),
    [repositories, runnerFilter, search.q],
  )
  const total = filteredRepositories.length
  const requestedPage = search.page ?? 1
  const page = Math.min(requestedPage, Math.max(1, Math.ceil(total / pageSize)))
  const visibleRepositories = paginateIntegrationRepositories(
    filteredRepositories,
    page,
    pageSize,
  )

  function updateSearch(updates: Partial<IntegrationDetailSearch>) {
    void navigate({
      search: (previous) => ({ ...previous, ...updates }),
      replace: true,
    })
  }

  usePageClamp(
    requestedPage,
    pageSize,
    repositoriesQuery.isLoading ? undefined : total,
    (nextPage) => {
      updateSearch({ page: nextPage === 1 ? undefined : nextPage })
    },
  )

  useBreadcrumbLabel(
    '/settings/integrations/$integrationId',
    detailQuery.data?.integration.display_name ??
      detailQuery.data?.integration.provider,
  )

  useMountEffect(() => {
    let handledCallback = false
    if (search.installed === 'true') {
      toast.success('GitHub App installed successfully')
      handledCallback = true
    }
    if (search.gitlab === 'success') {
      toast.success('GitLab OAuth authorization completed')
      handledCallback = true
    }
    if (handledCallback) {
      updateSearch({ installed: undefined, gitlab: undefined })
    }
  })

  function handleSync() {
    const provider = detailQuery.data?.integration.provider
    syncMutation.mutate(integrationId, {
      onSuccess: () => {
        toast.success(
          provider === 'gitlab'
            ? 'GitLab projects synced'
            : 'GitHub repositories synced',
        )
      },
      onError: (error) => {
        toast.error(`Sync failed: ${error.message}`)
      },
    })
  }

  function handleDisconnect() {
    const name =
      detailQuery.data?.integration.display_name ??
      detailQuery.data?.integration.provider ??
      'source'
    deleteMutation.mutate(integrationId, {
      onSuccess: () => {
        toast.success(`Disconnected source: ${name}`)
        void navigate({ to: '/settings/integrations' })
      },
      onError: (error) => {
        toast.error(`Failed to disconnect: ${error.message}`)
      },
    })
  }

  if (detailQuery.isLoading) {
    return { status: 'loading' as const, label }
  }

  if (detailQuery.error) {
    return {
      status: 'error' as const,
      label,
      message: detailQuery.error.message,
      retry: detailQuery.refetch,
    }
  }

  if (!detailQuery.data) return { status: 'missing' as const, label }

  const { integration } = detailQuery.data
  const installations = installationsQuery.data?.installations ?? []
  const providerLabel =
    integration.provider === 'gitlab'
      ? 'GitLab'
      : integration.provider === 'github'
        ? 'GitHub'
        : 'Local Git'
  const installationsLabel =
    integration.provider === 'gitlab'
      ? 'GitLab accounts'
      : integration.provider === 'github'
        ? 'Installations'
        : 'Local paths'
  const accountsTabLabel =
    integration.provider === 'local_git' ? 'Path' : 'Accounts'
  const accountsEmptyDescription =
    integration.provider === 'gitlab'
      ? 'Authorize GitLab, then sync this source.'
      : integration.provider === 'github'
        ? 'Install the GitHub App, then sync this source.'
        : 'This source has no linked local path.'
  const syncLabel =
    integration.provider === 'gitlab' ? 'Sync projects' : 'Sync repositories'
  const canSyncInstallations =
    integration.provider === 'github' || integration.provider === 'gitlab'
  const gitLabWebhookUrl = networkSettingsQuery.data
    ? gitLabPublicEndpoints(
        networkSettingsQuery.data.settings.public_url,
        window.location.origin,
      ).webhookUrl
    : null

  return {
    status: 'ready' as const,
    allowedRepositoryCount: repositories.filter(
      (repository) => repository.allow_direct_macos_runner,
    ).length,
    accountsTabLabel,
    accountsEmptyDescription,
    canSyncInstallations,
    deleteMutation,
    detail: detailQuery.data,
    disconnectImpactQuery,
    disconnectPrerequisiteError,
    gitLabWebhookUrl,
    gitlabAuthorizeMutation,
    handleDisconnect,
    handleSync,
    installations,
    installationsLabel,
    installationsQuery,
    integration,
    integrationId,
    label,
    networkSettingsQuery,
    page,
    pageSize,
    defaultPageSize,
    providerLabel,
    repositories,
    repositoriesQuery,
    runnerFilter,
    search,
    syncLabel,
    syncMutation,
    tab: search.tab ?? 'repositories',
    total,
    updateSearch,
    visibleRepositories,
  }
}

function IntegrationDetailPage() {
  const canWrite = useHasPermission('integrations', 'write')
  const [disconnectOpen, setDisconnectOpen] = useState(false)
  const [webhookTarget, setWebhookTarget] =
    useState<IntegrationRepository | null>(null)
  const pageState = useIntegrationDetailPageState(canWrite, disconnectOpen)

  if (pageState.status === 'loading') {
    return (
      <PageLayout width="wide">
        <PageMeta title={pageState.label} noindex />
        <Skeleton className="h-8 w-56" />
        <Skeleton className="h-10 w-72" />
        <Skeleton className="h-64 w-full" />
      </PageLayout>
    )
  }

  if (pageState.status === 'error') {
    return (
      <PageLayout width="wide">
        <PageMeta title={pageState.label} noindex />
        <Alert variant="destructive">
          <DynamicLucideIcon icon={InformationCircleIcon} size={16} />
          <AlertDescription>
            <span>Failed to load source: {pageState.message}</span>
            <Button
              type="button"
              variant="outline"
              size="sm"
              className="mt-3"
              onClick={() => void pageState.retry()}
            >
              Retry
            </Button>
          </AlertDescription>
        </Alert>
      </PageLayout>
    )
  }

  if (pageState.status === 'missing') {
    return (
      <PageLayout width="wide">
        <PageMeta title="Source not found" noindex />
        <PageHeader title="Source not found" />
        <p className="text-sm text-muted-foreground">
          This source connection does not exist or is no longer available.
        </p>
      </PageLayout>
    )
  }

  const {
    accountsEmptyDescription,
    allowedRepositoryCount,
    accountsTabLabel,
    canSyncInstallations,
    defaultPageSize,
    deleteMutation,
    detail,
    disconnectImpactQuery,
    disconnectPrerequisiteError,
    gitLabWebhookUrl,
    gitlabAuthorizeMutation,
    handleDisconnect,
    handleSync,
    installations,
    installationsLabel,
    installationsQuery,
    integration,
    integrationId,
    label,
    networkSettingsQuery,
    page,
    pageSize,
    providerLabel,
    repositories,
    repositoriesQuery,
    runnerFilter,
    search,
    syncLabel,
    syncMutation,
    tab,
    total,
    updateSearch,
    visibleRepositories,
  } = pageState
  const needsGitLabAuthorization =
    integration.provider === 'gitlab' &&
    integration.auth_mode === 'oauth_app' &&
    integration.status === 'inactive'
  const manageHref =
    integration.provider === 'github' && integration.app_slug
      ? installations.length > 0
        ? `https://github.com/apps/${integration.app_slug}/installations/select_target`
        : `https://github.com/apps/${integration.app_slug}/installations/new`
      : integration.provider === 'gitlab'
        ? integration.host_url
        : null
  const manageLabel =
    integration.provider === 'github'
      ? installations.length > 0
        ? 'Manage on GitHub'
        : 'Install on GitHub'
      : 'Open GitLab'

  return (
    <PageLayout width="wide">
      <PageMeta title={label} noindex />
      <PageHeader
        title={integration.display_name ?? integration.provider}
        description={`${providerLabel} source at ${integration.host_url}`}
        meta={
          <>
            <Badge variant={getIntegrationStatusVariant(integration.status)}>
              {integration.status}
            </Badge>
            <Badge variant="outline">{providerLabel}</Badge>
            {!repositoriesQuery.isLoading && !repositoriesQuery.error ? (
              <>
                <span>
                  {repositories.length}{' '}
                  {repositories.length === 1 ? 'repository' : 'repositories'}
                </span>
                <span>{allowedRepositoryCount} allowed</span>
              </>
            ) : null}
          </>
        }
        actions={
          canWrite ? (
            <IntegrationHeaderActions
              authorizePending={gitlabAuthorizeMutation.isPending}
              canSync={canSyncInstallations}
              manageHref={manageHref}
              manageLabel={manageLabel}
              needsAuthorization={needsGitLabAuthorization}
              onAuthorize={() =>
                gitlabAuthorizeMutation.mutate(
                  {
                    integration_id: integrationId,
                    redirect_url: window.location.href,
                  },
                  {
                    onError: (authorizationError) =>
                      toast.error(
                        `GitLab authorization failed: ${authorizationError.message}`,
                      ),
                  },
                )
              }
              onDisconnect={() => setDisconnectOpen(true)}
              onSync={handleSync}
              syncLabel={syncLabel}
              syncPending={syncMutation.isPending}
            />
          ) : null
        }
      />

      {!canWrite ? (
        <Alert>
          <AlertDescription>
            You have read-only access to this source connection.
          </AlertDescription>
        </Alert>
      ) : null}

      <Tabs
        value={tab}
        onValueChange={(value) =>
          updateSearch({
            tab:
              value === 'repositories'
                ? undefined
                : (value as Exclude<IntegrationDetailTab, 'repositories'>),
          })
        }
      >
        <TabsList variant="line" aria-label="Source details">
          <TabsTrigger value="repositories">
            Repositories
            <span className="font-mono text-xs text-muted-foreground">
              {repositoriesQuery.isLoading ? '…' : repositories.length}
            </span>
          </TabsTrigger>
          <TabsTrigger value="accounts">
            {accountsTabLabel}
            <span className="font-mono text-xs text-muted-foreground">
              {installationsQuery.isLoading ? '…' : installations.length}
            </span>
          </TabsTrigger>
          <TabsTrigger value="connection">Connection</TabsTrigger>
        </TabsList>

        <TabsContent value="repositories" className="pt-4">
          {integration.provider === 'gitlab' &&
          canWrite &&
          networkSettingsQuery.error ? (
            <Alert variant="destructive" className="mb-4">
              <DynamicLucideIcon icon={InformationCircleIcon} size={16} />
              <AlertDescription className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
                <span>
                  Webhook actions are unavailable because the public URL could
                  not be loaded.
                </span>
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => void networkSettingsQuery.refetch()}
                >
                  Retry
                </Button>
              </AlertDescription>
            </Alert>
          ) : null}
          <IntegrationRepositoryInventory
            canWrite={canWrite}
            error={repositoriesQuery.error}
            integration={integration}
            isLoading={repositoriesQuery.isLoading}
            onClearFilters={() =>
              updateSearch({ q: undefined, runner: undefined, page: undefined })
            }
            onPageChange={(nextPage) =>
              updateSearch({ page: nextPage === 1 ? undefined : nextPage })
            }
            onPageSizeChange={(nextPageSize) =>
              updateSearch({
                pageSize:
                  nextPageSize === defaultPageSize
                    ? undefined
                    : (nextPageSize as 10 | 20 | 50 | 100),
                page: undefined,
              })
            }
            onRetry={() => void repositoriesQuery.refetch()}
            onRunnerFilterChange={(nextFilter) =>
              updateSearch({
                runner: nextFilter === 'all' ? undefined : nextFilter,
                page: undefined,
              })
            }
            onSearch={(nextQuery) =>
              updateSearch({
                q: nextQuery.trim() || undefined,
                page: undefined,
              })
            }
            onWebhookTokenRequest={
              integration.provider === 'gitlab' && gitLabWebhookUrl
                ? setWebhookTarget
                : undefined
            }
            page={page}
            pageSize={pageSize}
            query={search.q}
            repositories={visibleRepositories}
            repositoryCount={repositories.length}
            runnerFilter={runnerFilter}
            total={total}
          />
        </TabsContent>

        <TabsContent value="accounts" className="pt-4">
          <IntegrationAccountsInventory
            emptyDescription={accountsEmptyDescription}
            error={installationsQuery.error}
            installations={installations}
            isLoading={installationsQuery.isLoading}
            label={installationsLabel}
            onRetry={() => void installationsQuery.refetch()}
            primaryColumnLabel={
              integration.provider === 'local_git' ? 'Path' : 'Account'
            }
          />
        </TabsContent>

        <TabsContent value="connection" className="pt-4">
          <IntegrationConnectionDetails
            canWrite={canWrite}
            gitLabWebhookUrl={gitLabWebhookUrl}
            integration={integration}
            lastWebhookAt={detail.last_webhook_at}
            networkSettingsError={networkSettingsQuery.error}
            networkSettingsLoading={networkSettingsQuery.isLoading}
            onRetryNetworkSettings={() => void networkSettingsQuery.refetch()}
          />
        </TabsContent>
      </Tabs>

      <IntegrationDisconnectDialog
        affectedProjects={disconnectImpactQuery.data ?? []}
        error={
          disconnectPrerequisiteError ??
          repositoriesQuery.error ??
          disconnectImpactQuery.error
        }
        integration={integration}
        isLoading={
          repositoriesQuery.isLoading || disconnectImpactQuery.isLoading
        }
        isPending={deleteMutation.isPending}
        onConfirm={handleDisconnect}
        onOpenChange={setDisconnectOpen}
        onRetry={
          disconnectPrerequisiteError
            ? undefined
            : () => {
                if (repositoriesQuery.error) {
                  void repositoriesQuery.refetch()
                  return
                }
                void disconnectImpactQuery.refetch()
              }
        }
        open={disconnectOpen}
        repositoryCount={repositories.length}
      />

      {integration.provider === 'gitlab' && canWrite && gitLabWebhookUrl ? (
        <GitLabWebhookTokenDialogs
          repository={webhookTarget}
          webhookUrl={gitLabWebhookUrl}
          onClose={() => setWebhookTarget(null)}
        />
      ) : null}
    </PageLayout>
  )
}
