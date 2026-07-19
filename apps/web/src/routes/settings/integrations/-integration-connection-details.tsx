import { Copy as Copy01Icon } from 'lucide-react'
import { DynamicLucideIcon } from '@/components/ui/dynamic-lucide-icon'

import { toast } from '@/lib/toast'
import type { Integration } from '@/lib/types'
import { Button } from '@/components/ui/button'
import { Skeleton } from '@/components/ui/skeleton'
import { Table, TableBody, TableCell, TableRow } from '@/components/ui/table'

function humanizeAuthMode(mode: string): string {
  const labels: Record<string, string> = {
    github_app_manifest: 'GitHub App manifest',
    github_app: 'GitHub App',
    oauth_app: 'OAuth app',
    pat: 'Personal access token',
    personal_token: 'Personal access token',
  }
  return (
    labels[mode] ??
    mode
      .replace(/_/g, ' ')
      .replace(/\b\w/g, (character) => character.toUpperCase())
  )
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
  lastWebhookAt: number | undefined
  networkSettingsError: Error | null
  networkSettingsLoading: boolean
  onRetryNetworkSettings: () => void
}) {
  return (
    <section className="border bg-card" aria-labelledby="connection-title">
      <div className="border-b px-4 py-3">
        <h2 id="connection-title" className="text-sm font-semibold">
          Connection
        </h2>
      </div>
      <div className="overflow-x-auto p-4">
        <Table>
          <TableBody>
            <TableRow>
              <TableCell className="w-56 text-muted-foreground">
                Provider
              </TableCell>
              <TableCell>{providerLabel(integration.provider)}</TableCell>
            </TableRow>
            <TableRow>
              <TableCell className="text-muted-foreground">Host URL</TableCell>
              <TableCell>{integration.host_url}</TableCell>
            </TableRow>
            <TableRow>
              <TableCell className="text-muted-foreground">Auth mode</TableCell>
              <TableCell>{humanizeAuthMode(integration.auth_mode)}</TableCell>
            </TableRow>
            {integration.provider === 'gitlab' && canWrite ? (
              <>
                <TableRow>
                  <TableCell className="text-muted-foreground">
                    Webhook URL
                  </TableCell>
                  <TableCell>
                    {networkSettingsLoading ? (
                      <Skeleton className="h-6 w-72" />
                    ) : networkSettingsError ? (
                      <div className="flex items-center gap-3">
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
                      </div>
                    ) : gitLabWebhookUrl ? (
                      <div className="flex items-center gap-2">
                        <code className="font-mono text-xs">
                          {gitLabWebhookUrl}
                        </code>
                        <Button
                          variant="ghost"
                          size="icon-sm"
                          aria-label="Copy GitLab webhook URL"
                          title="Copy GitLab webhook URL"
                          onClick={() => {
                            void navigator.clipboard
                              .writeText(gitLabWebhookUrl)
                              .then(
                                () => toast.success('Webhook URL copied'),
                                () => toast.error('Could not copy webhook URL'),
                              )
                          }}
                        >
                          <DynamicLucideIcon icon={Copy01Icon} />
                        </Button>
                      </div>
                    ) : null}
                  </TableCell>
                </TableRow>
                <TableRow>
                  <TableCell className="text-muted-foreground">
                    Last delivery for this source
                  </TableCell>
                  <TableCell>
                    {lastWebhookAt
                      ? new Date(lastWebhookAt * 1000).toLocaleString()
                      : 'No delivery received'}
                  </TableCell>
                </TableRow>
              </>
            ) : null}
            {integration.app_id ? (
              <TableRow>
                <TableCell className="text-muted-foreground">App ID</TableCell>
                <TableCell className="font-mono text-xs">
                  {integration.app_id}
                </TableCell>
              </TableRow>
            ) : null}
            <TableRow>
              <TableCell className="text-muted-foreground">Created</TableCell>
              <TableCell>
                {new Date(integration.created_at * 1000).toLocaleString()}
              </TableCell>
            </TableRow>
          </TableBody>
        </Table>
      </div>
    </section>
  )
}
