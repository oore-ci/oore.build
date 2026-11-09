import { HugeiconsIcon } from '@hugeicons/react'
import { Copy01Icon } from '@hugeicons/core-free-icons'
import { useMemo, type ReactNode } from 'react'

import { toast } from '@/lib/toast'
import type { Integration } from '@oore/client/models'
import {
  DataTable,
  useDataTable,
  type DataTableColumnDef,
} from '@/components/data-table'
import { Button } from '@/components/ui/button'
import { RelativeTime } from '@/components/relative-time'
import { formatEnumLabel } from '@/lib/format-utils'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Skeleton } from '@/components/ui/skeleton'
import { Row } from '@/components/layout/row'

interface ConnectionDetail {
  label: string
  value: ReactNode
}

const connectionColumns: Array<DataTableColumnDef<ConnectionDetail>> = [
  {
    accessorKey: 'label',
    header: 'Field',
  },
  {
    accessorKey: 'value',
    header: 'Value',
    cell: ({ row }) => row.original.value,
  },
]

function humanizeAuthMode(mode: string): string {
  const labels = new Map([
    ['github_app_manifest', 'GitHub App manifest'],
    ['github_app', 'GitHub App'],
    ['oauth_app', 'OAuth app'],
    ['pat', 'Personal access token'],
    ['personal_token', 'Personal access token'],
  ])
  return labels.get(mode) ?? formatEnumLabel(mode)
}

function providerLabel(provider: Integration['provider']): string {
  if (provider === 'github') return 'GitHub'
  if (provider === 'gitlab') return 'GitLab'
  return 'Local Git'
}

export function IntegrationConnectionDetails({
  canWrite,
  gitLabWebhookUrl,
  integration,
  lastWebhookAt,
  networkSettingsError,
  networkSettingsLoading,
  onRetryNetworkSettings,
}: {
  canWrite: boolean
  gitLabWebhookUrl: string | null
  integration: Integration
  lastWebhookAt: number | null | undefined
  networkSettingsError: Error | null
  networkSettingsLoading: boolean
  onRetryNetworkSettings: () => void
}) {
  const details = useMemo<Array<ConnectionDetail>>(() => {
    const rows: Array<ConnectionDetail> = [
      { label: 'Provider', value: providerLabel(integration.provider) },
      { label: 'Host URL', value: integration.host_url },
      { label: 'Auth mode', value: humanizeAuthMode(integration.auth_mode) },
    ]

    if (integration.provider === 'gitlab' && canWrite) {
      rows.push({
        label: 'Webhook URL',
        value: networkSettingsLoading ? (
          <Skeleton className="h-6 w-72" />
        ) : networkSettingsError ? (
          <Row gap={3}>
            <span className="text-sm text-destructive">
              Could not load webhook URL
            </span>
            <Button
              variant="outline"
              size="sm"
              onClick={onRetryNetworkSettings}
            >
              Retry
            </Button>
          </Row>
        ) : gitLabWebhookUrl ? (
          <Row gap={2}>
            <code className="font-mono text-xs">{gitLabWebhookUrl}</code>
            <Button
              variant="ghost"
              size="icon-sm"
              aria-label="Copy GitLab webhook URL"
              title="Copy GitLab webhook URL"
              onClick={() => {
                void navigator.clipboard.writeText(gitLabWebhookUrl).then(
                  () => toast.success('Webhook URL copied'),
                  () => toast.error('Could not copy webhook URL'),
                )
              }}
            >
              <HugeiconsIcon icon={Copy01Icon} />
            </Button>
          </Row>
        ) : null,
      })
      rows.push({
        label: 'Last delivery for this source',
        value: lastWebhookAt ? (
          <RelativeTime at={lastWebhookAt} />
        ) : (
          'No delivery received'
        ),
      })
    }

    if (integration.app_id) {
      rows.push({
        label: 'App ID',
        value: <code className="font-mono text-xs">{integration.app_id}</code>,
      })
    }

    rows.push({
      label: 'Created',
      value: <RelativeTime at={integration.created_at} />,
    })
    return rows
  }, [
    canWrite,
    gitLabWebhookUrl,
    integration,
    lastWebhookAt,
    networkSettingsError,
    networkSettingsLoading,
    onRetryNetworkSettings,
  ])
  const table = useDataTable({
    columns: connectionColumns,
    data: details,
    getRowId: (row) => row.label,
  })

  return (
    <Card size="sm" aria-labelledby="connection-title">
      <CardHeader>
        <CardTitle id="connection-title">Connection</CardTitle>
      </CardHeader>
      <CardContent>
        <DataTable table={table} />
      </CardContent>
    </Card>
  )
}
