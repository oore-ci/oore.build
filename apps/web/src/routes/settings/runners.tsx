import { createMemo, Show } from 'solid-js'
import { createFileRoute, redirect } from '@tanstack/solid-router'

import type { Runner } from '@/lib/types'
import {
  getActiveInstanceOrRedirect,
  requireAuthOrRedirect,
} from '@/lib/instance-context'
import { useAuthStore } from '@/stores/auth-store'
import { useRunners, useUpdateRunner } from '@/hooks/use-runners'
import { getRunnerStatusVariant } from '@/lib/status-variants'
import { PageMeta } from '@/lib/seo'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { PageHeader } from '@/components/page-header'
import { PageLayout } from '@/components/page-layout'
import { Skeleton } from '@/components/ui/skeleton'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import { toast } from '@/components/ui/sonner'

export const Route = createFileRoute('/settings/runners')({
  staticData: { breadcrumbLabel: 'Runners' },
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireAuthOrRedirect(instance.id)

    const user = useAuthStore.getState().user
    if (!user || (user.role !== 'owner' && user.role !== 'admin')) {
      throw redirect({ to: '/' })
    }
  },
  component: RunnersSettingsPage,
})

function formatRelativeTime(epochSeconds?: number): string {
  if (!epochSeconds) return 'never'
  const diffSecs = Math.floor(Date.now() / 1000) - epochSeconds
  if (diffSecs < 5) return 'just now'
  if (diffSecs < 60) return `${diffSecs}s ago`
  const mins = Math.floor(diffSecs / 60)
  if (mins < 60) return `${mins}m ago`
  const hrs = Math.floor(mins / 60)
  if (hrs < 24) return `${hrs}h ago`
  const days = Math.floor(hrs / 24)
  return `${days}d ago`
}

function formatCapabilities(capabilities: Runner['capabilities']): string {
  const entries = Object.entries(capabilities)
  if (entries.length === 0) return 'none'
  return entries
    .slice(0, 3)
    .map(([key, value]) => `${key}:${String(value)}`)
    .join(', ')
}

function RunnersSettingsPage() {
  const runnersQuery = useRunners()
  const updateRunner = useUpdateRunner()
  const user = useAuthStore((state) => state.user)
  const canWrite = createMemo(
    () => user()?.role === 'owner' || user()?.role === 'admin',
  )

  const runners = () => runnersQuery.data?.runners ?? []
  const onlineCount = createMemo(
    () =>
      runners().filter(
        (runner) => runner.status === 'online' || runner.status === 'busy',
      ).length,
  )

  const handleRename = (runner: Runner) => {
    if (!canWrite()) return
    if (!runner.registered_by) {
      toast.message(
        'Embedded runner names are managed by the daemon and cannot be changed.',
      )
      return
    }

    const nextName = window.prompt('Runner name', runner.name)?.trim()
    if (!nextName || nextName === runner.name) return

    updateRunner.mutate(
      { runnerId: runner.id, data: { name: nextName } },
      {
        onSuccess: () => toast.success('Runner renamed'),
        onError: (error) =>
          toast.error(error instanceof Error ? error.message : 'Rename failed'),
      },
    )
  }

  return (
    <PageLayout class="space-y-4">
      <PageMeta title="Runner Management" noindex />
      <PageHeader
        title="Runners"
        description="Runner health and metadata management for this instance."
      />

      <section class="grid gap-4 md:grid-cols-3">
        <Card>
          <CardContent class="pt-6">
            <p class="text-xs font-medium uppercase tracking-wider text-muted-foreground">
              Total runners
            </p>
            <p class="mt-3 text-2xl font-bold tracking-tight">{runners().length}</p>
            <p class="mt-1 text-xs text-muted-foreground">Embedded and external</p>
          </CardContent>
        </Card>
        <Card>
          <CardContent class="pt-6">
            <div class="flex items-center justify-between">
              <p class="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                Online runners
              </p>
              <Show when={onlineCount() > 0}>
                <Badge variant="default">{onlineCount()}</Badge>
              </Show>
            </div>
            <p class="mt-3 text-2xl font-bold tracking-tight">{onlineCount()}</p>
            <p class="mt-1 text-xs text-muted-foreground">online or busy</p>
          </CardContent>
        </Card>
        <Card>
          <CardContent class="pt-6">
            <p class="text-xs font-medium uppercase tracking-wider text-muted-foreground">
              Rename policy
            </p>
            <p class="mt-3 text-sm font-medium">
              External only
            </p>
            <p class="mt-1 text-xs text-muted-foreground">
              Embedded runners stay daemon-managed
            </p>
          </CardContent>
        </Card>
      </section>

      <Show when={runnersQuery.isLoading}>
        <Card>
          <CardContent class="space-y-2 pt-6">
            <Skeleton class="h-10 w-full" />
            <Skeleton class="h-10 w-full" />
            <Skeleton class="h-10 w-full" />
          </CardContent>
        </Card>
      </Show>

      <Show when={runnersQuery.error}>
        <Alert variant="destructive">
          <AlertDescription>
            Failed to load runners: {runnersQuery.error?.message}
          </AlertDescription>
        </Alert>
      </Show>

      <Show when={!runnersQuery.isLoading && !runnersQuery.error}>
        <Card>
          <CardHeader>
            <CardTitle class="text-base">Runner inventory</CardTitle>
          </CardHeader>
          <CardContent>
            <Show
              when={runners().length > 0}
              fallback={
                <p class="py-6 text-sm text-muted-foreground">
                  No runners registered yet.
                </p>
              }
            >
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>Name</TableHead>
                    <TableHead>Status</TableHead>
                    <TableHead>Last heartbeat</TableHead>
                    <TableHead>Capabilities</TableHead>
                    <TableHead>Registered by</TableHead>
                    <TableHead class="text-right">Actions</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {runners().map((runner) => (
                    <TableRow>
                      <TableCell>
                        <p class="font-medium">{runner.name}</p>
                        <p class="text-xs text-muted-foreground font-mono">
                          {runner.id.slice(0, 8)}
                        </p>
                      </TableCell>
                      <TableCell>
                        <Badge variant={getRunnerStatusVariant(runner.status)}>
                          {runner.status}
                        </Badge>
                      </TableCell>
                      <TableCell class="text-xs text-muted-foreground">
                        {formatRelativeTime(runner.last_heartbeat_at)}
                      </TableCell>
                      <TableCell class="text-xs text-muted-foreground">
                        {formatCapabilities(runner.capabilities)}
                      </TableCell>
                      <TableCell class="text-xs text-muted-foreground">
                        {runner.registered_by ?? 'daemon-managed'}
                      </TableCell>
                      <TableCell class="text-right">
                        <Button
                          variant="outline"
                          size="sm"
                          onClick={() => handleRename(runner)}
                          disabled={updateRunner.isPending || !canWrite()}
                        >
                          Rename
                        </Button>
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            </Show>
          </CardContent>
        </Card>
      </Show>
    </PageLayout>
  )
}
