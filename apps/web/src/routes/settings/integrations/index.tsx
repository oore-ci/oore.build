import { useMemo } from 'react'
import { Link, createFileRoute, useSearch } from '@tanstack/react-router'
import { DynamicLucideIcon } from '@/components/ui/dynamic-lucide-icon'
import {
  Info as InformationCircleIcon,
  Link2 as Link04Icon,
} from 'lucide-react'
import { toast } from '@/lib/toast'

import { useMountEffect } from '@/hooks/use-mount-effect'
import { usePageClamp } from '@/hooks/use-page-clamp'
import {
  getActiveInstanceOrRedirect,
  requireInstanceRoleOrRedirect,
} from '@/lib/instance-context'
import { useHasPermission } from '@/hooks/use-permissions'
import { useInstancePreferences } from '@/hooks/use-artifact-storage'
import { useIntegrations } from '@/hooks/use-integrations'
import { PageMeta } from '@/lib/seo'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { Skeleton } from '@/components/ui/skeleton'
import type { SortDirection } from '@/components/collection-controls'
import PageHeader from '@/components/page-header'
import PageLayout from '@/components/page-layout'
import type { IntegrationSort } from './-source-inventory'
import { ConnectSourceOptions } from './-connect-source-options'
import { ConnectedSourcesSection } from './-connected-sources-section'
import { LocalOnlySourcesNotice } from './-local-only-sources-notice'

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
  staticData: { breadcrumb: {
   title: 'Sources' ,
 },},
  validateSearch: parseSearch,
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireInstanceRoleOrRedirect(instance.id, ['owner', 'admin', 'developer'])
  },
  component: IntegrationsPage,
})

function IntegrationsPage() {
  const canWrite = useHasPermission('integrations', 'write')
  const search = useSearch({ from: '/settings/integrations/' })
  const navigate = Route.useNavigate()
  const integrationsQuery = useIntegrations()
  const preferencesQuery = useInstancePreferences({ enabled: canWrite })
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

  const hasSearch = !!search.q
  const sourceCount = integrationsQuery.data?.integrations.length ?? 0
  const hasConnectedSources = sourceCount > 0
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
                <DynamicLucideIcon
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
          <DynamicLucideIcon icon={InformationCircleIcon} size={16} />
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
        <LocalOnlySourcesNotice
          actions={
            <>
              <Button
                variant="outline"
                render={<Link to="/settings/preferences" />}
                nativeButton={false}
              >
                Open General settings
              </Button>
              <Button render={<Link to="/projects" />} nativeButton={false}>
                Go to projects
              </Button>
            </>
          }
        />
      ) : null}

      {remoteEnabled &&
      (integrationsQuery.isLoading ||
        integrationsQuery.error ||
        hasConnectedSources ||
        hasSearch ||
        !canWrite) ? (
        <ConnectedSourcesSection
          collection={{
            canWrite,
            integrations: visibleIntegrations,
            total,
          }}
          direction={direction}
          onClearSearch={() => updateSearch({ q: undefined, page: undefined })}
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
          onRetry={() => void integrationsQuery.refetch()}
          onSearch={(value) =>
            updateSearch({ q: value.trim() || undefined, page: undefined })
          }
          onSortChange={handleSortChange}
          page={page}
          pageSize={pageSize}
          query={{
            error: integrationsQuery.error,
            isLoading: integrationsQuery.isLoading,
            search: search.q,
          }}
          sort={sort}
        />
      ) : null}

      {remoteEnabled &&
      canWrite &&
      !integrationsQuery.isLoading &&
      !integrationsQuery.error &&
      !hasConnectedSources ? (
        <ConnectSourceOptions />
      ) : null}

      {remoteEnabled && !canWrite ? (
        <Alert>
          <DynamicLucideIcon icon={InformationCircleIcon} size={16} />
          <AlertDescription>
            You have read-only access to connected sources. An owner or admin
            can add, reconnect, or disconnect providers.
          </AlertDescription>
        </Alert>
      ) : null}
    </PageLayout>
  )
}
