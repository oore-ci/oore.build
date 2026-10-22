import { useMemo, useState } from 'react'
import { createFileRoute, useSearch } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  Delete02Icon,
  InformationCircleIcon,
  LinkSquare02Icon,
  MoreHorizontalCircle01Icon,
  Refresh01Icon,
  Setting07Icon,
} from '@hugeicons/core-free-icons'

import { toast } from '@/lib/toast'
import { searchNumber, searchString } from '@/lib/search-input'
import type { SearchInput } from '@/lib/search-input'
import { useMountEffect } from '@/hooks/use-mount-effect'
import { usePageClamp } from '@/hooks/use-page-clamp'
import { useIsBelowBreakpoint } from '@/hooks/use-mobile'
import {
  getActiveInstanceOrRedirect,
  requireInstanceRoleOrRedirect,
} from '@/lib/instance-context'
import { useHasPermission } from '@/hooks/use-permissions'
import {
  useMarkOperatorIncidentRead,
  useOperatorIncidents,
} from '@/hooks/use-operator-incidents'
import {
  useDeleteIntegration,
  useGitLabAuthorize,
  useGitLabTokenStatus,
  useInstallations,
  useIntegration,
  useIntegrationRepos,
  useSyncInstallations,
} from '@/hooks/use-integrations'
import { StatusBadge } from '@/components/status-badge'
import { useExternalAccessNetworkSettings } from '@/hooks/use-artifact-storage'
import { useInfiniteProjects } from '@/hooks/use-projects'
import { gitLabPublicEndpoints } from '@/lib/gitlab-url'
import { PageMeta } from '@/lib/seo'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import PageHeader from '@/components/page-header'
import PageLayout from '@/components/page-layout'
import { Skeleton } from '@/components/ui/skeleton'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import type { IntegrationRepository } from '@oore/client/models'
import {
  IntegrationAccountsInventory,
  IntegrationRepositoryInventory,
} from './-integration-inventory'
import { GitLabWebhookTokenDialogs } from './-gitlab-webhook-tokens'
import { GitLabTokenSettings } from './-gitlab-token-settings'
import { IntegrationConnectionDetails } from './-integration-connection-details'
import { IntegrationDisconnectDialog } from './-integration-disconnect-dialog'
import { OperatorIncidentAlert } from '@/components/operator-incident-alert'
import { SourceStatusAlert } from './-source-status-alert'

type IntegrationDetailTab = 'repositories' | 'accounts' | 'connection'

const EMPTY_REPOSITORIES: Array<IntegrationRepository> = []

interface IntegrationDetailSearch {
  gitlab?: string
  installed?: string
  page?: number
  pageSize?: 10 | 20 | 50 | 100
  q?: string
  tab?: Exclude<IntegrationDetailTab, 'repositories'>
}

function parseSearch(search: SearchInput): IntegrationDetailSearch {
  const page = searchNumber(search, 'page')
  const pageSize = searchNumber(search, 'pageSize')
  const q = searchString(search, 'q')?.trim() ?? ''
  const tab = searchString(search, 'tab')

  return {
    installed: searchString(search, 'installed'),
    gitlab: searchString(search, 'gitlab'),
    q: q || undefined,
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
    breadcrumb: {
      title: 'Source',
    },
  },
  validateSearch: parseSearch,
  beforeLoad: (context) => {
    const instance = getActiveInstanceOrRedirect()
    requireInstanceRoleOrRedirect(
      instance.id,
      ['owner', 'admin', 'developer'],
      context,
    )
  },
  component: IntegrationDetailPage,
})

function IntegrationDetailPage() {
  const canWrite = useHasPermission('integrations:write')
  const [disconnectOpen, setDisconnectOpen] = useState(false)
  const [webhookTarget, setWebhookTarget] =
    useState<IntegrationRepository | null>(null)
  const { integrationId } = Route.useParams()
  const search = useSearch({ from: '/settings/integrations/$integrationId' })
  const navigate = Route.useNavigate()

  const detailQuery = useIntegration(integrationId)
  const installationsQuery = useInstallations(integrationId)
  const isCompact = useIsBelowBreakpoint(640)
  const defaultPageSize = isCompact ? 10 : 20
  const pageSize = search.pageSize ?? defaultPageSize
  const requestedPage = search.page ?? 1
  const repositoriesQuery = useIntegrationRepos(integrationId, {
    q: search.q,
    limit: pageSize,
    offset: (requestedPage - 1) * pageSize,
  })
  const networkSettingsQuery = useExternalAccessNetworkSettings({
    enabled: canWrite,
  })
  const syncMutation = useSyncInstallations()
  const deleteMutation = useDeleteIntegration()
  const gitlabAuthorizeMutation = useGitLabAuthorize()
  const incidentsQuery = useOperatorIncidents({
    enabled: canWrite,
    resourceId: integrationId,
  })
  const markIncidentRead = useMarkOperatorIncidentRead()
  // Fetched only on demand, when an operator asks Oore to check the token.
  const tokenCheckQuery = useGitLabTokenStatus(integrationId, false)
  // Unfiltered count, so a search does not change the repository total.
  const repositoryTotalQuery = useIntegrationRepos(integrationId, { limit: 1 })
  const repositoryTotal = repositoryTotalQuery.data?.total

  const label =
    detailQuery.data?.integration.display_name ??
    detailQuery.data?.integration.provider ??
    'Source Details'
  const repositories =
    repositoriesQuery.data?.repositories ?? EMPTY_REPOSITORIES
  const disconnectImpactQuery = useInfiniteProjects(
    {
      integration_id: integrationId,
      limit: 100,
      sort: 'name',
      direction: 'asc',
    },
    {
      enabled:
        disconnectOpen &&
        canWrite &&
        !repositoriesQuery.isLoading &&
        !repositoriesQuery.error,
    },
  )
  const affectedProjects = useMemo(
    () =>
      disconnectImpactQuery.data?.pages.flatMap((page) => page.projects) ?? [],
    [disconnectImpactQuery.data?.pages],
  )
  const affectedProjectTotal = disconnectImpactQuery.data?.pages[0]?.total ?? 0
  const total = repositoriesQuery.data?.total ?? 0
  const page = Math.min(requestedPage, Math.max(1, Math.ceil(total / pageSize)))
  const visibleRepositories = repositories

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
            ? 'GitLab repositories synced'
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
    return (
      <PageLayout width="wide">
        <PageMeta title={label} noindex />
        <Skeleton className="h-8 w-56" />
        <Skeleton className="h-10 w-72" />
        <Skeleton className="h-64 w-full" />
      </PageLayout>
    )
  }

  if (detailQuery.error) {
    return (
      <PageLayout width="wide">
        <PageMeta title={label} noindex />
        <Alert variant="destructive">
          <HugeiconsIcon icon={InformationCircleIcon} size={16} />
          <AlertDescription>
            <span>Failed to load source: {detailQuery.error.message}</span>
            <Button
              type="button"
              variant="outline"
              size="sm"
              className="mt-3"
              onClick={() => void detailQuery.refetch()}
            >
              Retry
            </Button>
          </AlertDescription>
        </Alert>
      </PageLayout>
    )
  }

  if (!detailQuery.data) {
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

  const detail = detailQuery.data
  const { integration } = detail
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
  const syncLabel = 'Sync repositories'
  const canSyncInstallations =
    integration.provider === 'github' || integration.provider === 'gitlab'
  const tab = search.tab ?? 'repositories'
  const gitLabWebhookUrl = networkSettingsQuery.data
    ? gitLabPublicEndpoints(
        networkSettingsQuery.data.public_url ?? undefined,
        window.location.origin,
      ).webhookUrl
    : null

  const needsGitLabAuthorization =
    integration.provider === 'gitlab' &&
    integration.auth_mode === 'oauth_app' &&
    (integration.status === 'inactive' || integration.status === 'error')
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
            <StatusBadge kind="integration" value={integration.status} />
            <Badge variant="outline">{providerLabel}</Badge>
            {repositoryTotal !== undefined ? (
              <span>
                {repositoryTotal}{' '}
                {repositoryTotal === 1 ? 'repository' : 'repositories'}
              </span>
            ) : null}
          </>
        }
        actions={
          canWrite ? (
            <>
              {needsGitLabAuthorization ? (
                <Button
                  onClick={() =>
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
                  disabled={gitlabAuthorizeMutation.isPending}
                >
                  <HugeiconsIcon icon={LinkSquare02Icon} size={16} />
                  {gitlabAuthorizeMutation.isPending
                    ? 'Redirecting...'
                    : 'Authorize GitLab'}
                </Button>
              ) : canSyncInstallations ? (
                <Button onClick={handleSync} disabled={syncMutation.isPending}>
                  <HugeiconsIcon icon={Refresh01Icon} />
                  {syncMutation.isPending ? 'Syncing...' : syncLabel}
                </Button>
              ) : null}

              <DropdownMenu>
                <DropdownMenuTrigger
                  render={
                    <Button
                      variant="outline"
                      size="icon"
                      aria-label="Source actions"
                    />
                  }
                >
                  <HugeiconsIcon icon={MoreHorizontalCircle01Icon} />
                </DropdownMenuTrigger>
                <DropdownMenuContent align="end" className="w-auto">
                  {manageHref ? (
                    <DropdownMenuItem
                      onClick={() =>
                        window.open(manageHref, '_blank', 'noopener,noreferrer')
                      }
                    >
                      <HugeiconsIcon icon={Setting07Icon} />
                      {manageLabel}
                    </DropdownMenuItem>
                  ) : null}
                  <DropdownMenuItem
                    variant="destructive"
                    onClick={() => setDisconnectOpen(true)}
                  >
                    <HugeiconsIcon icon={Delete02Icon} />
                    Disconnect source
                  </DropdownMenuItem>
                </DropdownMenuContent>
              </DropdownMenu>
            </>
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

      {!canWrite ||
      (incidentsQuery.isSuccess &&
        incidentsQuery.data.incidents.length === 0) ? (
        <SourceStatusAlert
          canWrite={canWrite}
          checkPending={tokenCheckQuery.isFetching}
          integration={integration}
          onCheckAgain={
            integration.auth_mode === 'personal_token'
              ? () =>
                  void tokenCheckQuery
                    .refetch()
                    .then(() => detailQuery.refetch())
              : undefined
          }
          onReconnect={
            integration.auth_mode === 'personal_token'
              ? () => updateSearch({ tab: 'connection' })
              : integration.provider === 'github' && manageHref
                ? () => window.open(manageHref, '_blank', 'noopener,noreferrer')
                : undefined
          }
          reconnectLabel={
            integration.auth_mode === 'personal_token'
              ? 'Replace token'
              : manageLabel
          }
        />
      ) : null}

      {incidentsQuery.data?.incidents.map((incident) => (
        <OperatorIncidentAlert
          incident={incident}
          key={incident.id}
          onRead={() => markIncidentRead.mutate(incident.id)}
        />
      ))}

      <Tabs
        value={tab}
        onValueChange={(value) => {
          if (value === 'repositories') updateSearch({ tab: undefined })
          if (value === 'accounts' || value === 'connection') {
            updateSearch({ tab: value })
          }
        }}
      >
        <TabsList variant="line" aria-label="Source details">
          <TabsTrigger value="repositories">
            Repositories
            <span className="font-mono text-xs text-muted-foreground">
              {repositoryTotal ?? '…'}
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
              <HugeiconsIcon icon={InformationCircleIcon} size={16} />
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
              updateSearch({ q: undefined, page: undefined })
            }
            onPageChange={(nextPage) =>
              updateSearch({ page: nextPage === 1 ? undefined : nextPage })
            }
            onRetry={() => void repositoriesQuery.refetch()}
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
          <div className="space-y-4">
            <IntegrationConnectionDetails
              canWrite={canWrite}
              gitLabWebhookUrl={gitLabWebhookUrl}
              integration={integration}
              lastWebhookAt={detail.last_webhook_at}
              networkSettingsError={networkSettingsQuery.error}
              networkSettingsLoading={networkSettingsQuery.isLoading}
              onRetryNetworkSettings={() => void networkSettingsQuery.refetch()}
            />
            {integration.provider === 'gitlab' &&
            integration.auth_mode === 'personal_token' ? (
              <GitLabTokenSettings
                canWrite={canWrite}
                integrationId={integration.id}
              />
            ) : null}
          </div>
        </TabsContent>
      </Tabs>

      <IntegrationDisconnectDialog
        affectedProjects={affectedProjects}
        affectedProjectTotal={affectedProjectTotal}
        error={repositoriesQuery.error ?? disconnectImpactQuery.error}
        hasMoreProjects={Boolean(disconnectImpactQuery.hasNextPage)}
        integration={integration}
        isLoading={
          repositoriesQuery.isLoading || disconnectImpactQuery.isLoading
        }
        isPending={deleteMutation.isPending}
        isFetchingMoreProjects={disconnectImpactQuery.isFetchingNextPage}
        onConfirm={handleDisconnect}
        onOpenChange={setDisconnectOpen}
        onLoadMoreProjects={() => void disconnectImpactQuery.fetchNextPage()}
        onRetry={() => {
          if (repositoriesQuery.error) {
            void repositoriesQuery.refetch()
            return
          }
          void disconnectImpactQuery.refetch()
        }}
        open={disconnectOpen}
        repositoryCount={repositoryTotal ?? total}
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
