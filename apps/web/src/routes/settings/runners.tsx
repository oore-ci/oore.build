import { useEffect, useMemo, useState } from 'react'
import { createFileRoute, redirect } from '@tanstack/react-router'
import { useForm } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import { z } from 'zod'
import { toast } from 'sonner'

import type { Runner } from '@/lib/types'
import { getActiveInstanceOrRedirect, requireAuthOrRedirect } from '@/lib/instance-context'
import { useAuthStore } from '@/stores/auth-store'
import { useHasPermission } from '@/hooks/use-permissions'
import { useRunners, useUpdateRunner } from '@/hooks/use-runners'
import { getRunnerStatusVariant } from '@/lib/status-variants'
import { webPageTitle } from '@/lib/seo'
import PageLayout from '@/components/page-layout'
import PageHeader from '@/components/page-header'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import {
  Form,
  FormControl,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
} from '@/components/ui/form'
import { Input } from '@/components/ui/input'
import { Skeleton } from '@/components/ui/skeleton'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import { Spinner } from '@/components/ui/spinner'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'

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
  const entries = Object.entries(capabilities ?? {})
  if (entries.length === 0) return 'none'
  return entries
    .slice(0, 3)
    .map(([key, value]) => `${key}:${String(value)}`)
    .join(', ')
}

const renameRunnerSchema = z.object({
  name: z
    .string()
    .trim()
    .min(1, 'Name is required')
    .max(255, 'Name must be at most 255 characters'),
})

type RenameRunnerForm = z.infer<typeof renameRunnerSchema>

interface RenameRunnerDialogProps {
  runner: Runner | null
  open: boolean
  onOpenChange: (open: boolean) => void
}

function RenameRunnerDialog({ runner, open, onOpenChange }: RenameRunnerDialogProps) {
  const mutation = useUpdateRunner()
  const form = useForm<RenameRunnerForm>({
    resolver: zodResolver(renameRunnerSchema),
    defaultValues: { name: runner?.name ?? '' },
    mode: 'onBlur',
  })

  const initialName = runner?.name ?? ''
  const isEmbedded = !runner?.registered_by

  useEffect(() => {
    form.reset({ name: runner?.name ?? '' })
  }, [runner, form])

  function handleClose(nextOpen: boolean) {
    if (!nextOpen) {
      form.reset({ name: runner?.name ?? '' })
    }
    onOpenChange(nextOpen)
  }

  function onSubmit(data: RenameRunnerForm) {
    if (!runner) return

    const trimmed = data.name.trim()
    if (trimmed === initialName.trim()) {
      handleClose(false)
      return
    }

    mutation.mutate(
      { runnerId: runner.id, data: { name: trimmed } },
      {
        onSuccess: () => {
          toast.success('Runner renamed')
          handleClose(false)
        },
        onError: (error) => {
          toast.error(error instanceof Error ? error.message : 'Failed to rename runner')
        },
      },
    )
  }

  return (
    <Dialog open={open} onOpenChange={handleClose}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Rename runner</DialogTitle>
          <DialogDescription>
            Update the display name for this runner.
          </DialogDescription>
        </DialogHeader>

        {isEmbedded ? (
          <Alert>
            <AlertDescription>
              Embedded runner names are managed by the daemon and cannot be changed.
            </AlertDescription>
          </Alert>
        ) : (
          <Form {...form}>
            <form onSubmit={form.handleSubmit(onSubmit)} className="space-y-4">
              <FormField
                control={form.control}
                name="name"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel>Name</FormLabel>
                    <FormControl>
                      <Input autoFocus placeholder="Runner name" {...field} />
                    </FormControl>
                    <FormMessage />
                  </FormItem>
                )}
              />

              <DialogFooter>
                <Button
                  type="button"
                  variant="outline"
                  onClick={() => handleClose(false)}
                >
                  Cancel
                </Button>
                <Button type="submit" disabled={mutation.isPending}>
                  {mutation.isPending ? (
                    <>
                      <Spinner className="size-4" />
                      Saving...
                    </>
                  ) : (
                    'Save'
                  )}
                </Button>
              </DialogFooter>
            </form>
          </Form>
        )}
      </DialogContent>
    </Dialog>
  )
}

function RunnersSettingsPage() {
  const { data, isLoading, error } = useRunners()
  const canWrite = useHasPermission('runners', 'write')
  const [selectedRunner, setSelectedRunner] = useState<Runner | null>(null)
  const [dialogOpen, setDialogOpen] = useState(false)

  useEffect(() => {
    document.title = webPageTitle('Runner Management')
  }, [])

  const runners = data?.runners ?? []
  const onlineCount = useMemo(
    () => runners.filter((runner) => runner.status === 'online' || runner.status === 'busy').length,
    [runners],
  )

  return (
    <PageLayout width="wide">
      <PageHeader
        title="Runners"
        description="Runner health and metadata management for this instance."
      />

      <section className="grid gap-4 md:grid-cols-3">
        <Card>
          <CardContent>
            <p className="text-xs font-medium uppercase tracking-wider text-muted-foreground">Total runners</p>
            <p className="mt-3 text-2xl font-bold tracking-tight">{runners.length}</p>
            <p className="mt-1 text-xs text-muted-foreground">Embedded and external</p>
          </CardContent>
        </Card>
        <Card>
          <CardContent>
            <div className="flex items-center justify-between">
              <p className="text-xs font-medium uppercase tracking-wider text-muted-foreground">Online runners</p>
              {onlineCount > 0 ? <Badge variant="success">{onlineCount}</Badge> : null}
            </div>
            <p className="mt-3 text-2xl font-bold tracking-tight">{onlineCount}</p>
            <p className="mt-1 text-xs text-muted-foreground">Online or currently busy</p>
          </CardContent>
        </Card>
        <Card>
          <CardContent>
            <p className="text-xs font-medium uppercase tracking-wider text-muted-foreground">Rename policy</p>
            <p className="mt-3 text-sm font-bold">External only</p>
            <p className="mt-1 text-xs text-muted-foreground">Embedded runners stay daemon-managed</p>
          </CardContent>
        </Card>
      </section>

      {isLoading ? (
        <Card>
          <CardContent className="space-y-3">
            <Skeleton className="h-10 w-full" />
            <Skeleton className="h-10 w-full" />
            <Skeleton className="h-10 w-full" />
          </CardContent>
        </Card>
      ) : null}

      {error ? (
        <Alert variant="destructive">
          <AlertDescription>
            Failed to load runners: {error.message}
          </AlertDescription>
        </Alert>
      ) : null}

      {!isLoading && !error ? (
        <Card>
          <CardHeader>
            <CardTitle className="text-sm font-medium uppercase tracking-wider text-muted-foreground">Runner Inventory</CardTitle>
          </CardHeader>
          <CardContent>
            {runners.length === 0 ? (
              <p className="py-6 text-sm text-muted-foreground">No runners registered yet.</p>
            ) : (
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>Name</TableHead>
                    <TableHead>Status</TableHead>
                    <TableHead>Last heartbeat</TableHead>
                    <TableHead>Capabilities</TableHead>
                    <TableHead>Registered by</TableHead>
                    <TableHead className="text-right">Action</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {runners.map((runner) => {
                    const isEmbedded = !runner.registered_by
                    const canRename = canWrite && !isEmbedded
                    return (
                      <TableRow key={runner.id}>
                        <TableCell>
                          <p className="font-medium">{runner.name}</p>
                          <p className="font-mono text-xs text-muted-foreground">{runner.id.slice(0, 8)}</p>
                        </TableCell>
                        <TableCell>
                          <Badge variant={getRunnerStatusVariant(runner.status)}>
                            {runner.status}
                          </Badge>
                        </TableCell>
                        <TableCell className="text-muted-foreground">
                          {formatRelativeTime(runner.last_heartbeat_at)}
                        </TableCell>
                        <TableCell className="text-xs text-muted-foreground">
                          {formatCapabilities(runner.capabilities)}
                        </TableCell>
                        <TableCell className="text-sm text-muted-foreground">
                          {runner.registered_by ?? 'embedded'}
                        </TableCell>
                        <TableCell className="text-right">
                          <Button
                            variant="outline"
                            size="sm"
                            disabled={!canRename}
                            onClick={() => {
                              setSelectedRunner(runner)
                              setDialogOpen(true)
                            }}
                          >
                            Rename
                          </Button>
                        </TableCell>
                      </TableRow>
                    )
                  })}
                </TableBody>
              </Table>
            )}
          </CardContent>
        </Card>
      ) : null}

      <RenameRunnerDialog
        open={dialogOpen}
        runner={selectedRunner}
        onOpenChange={(open) => {
          setDialogOpen(open)
          if (!open) {
            setSelectedRunner(null)
          }
        }}
      />
    </PageLayout>
  )
}
