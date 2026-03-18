import { createFileRoute, redirect } from '@tanstack/react-router'
import { useForm } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import z from 'zod'
import { toast } from 'sonner'

import type { RetentionCleanupTarget } from '@/lib/types'
import {
  getActiveInstanceOrRedirect,
  requireAuthOrRedirect,
} from '@/lib/instance-context'
import { useAuthStore } from '@/stores/auth-store'
import { PageMeta } from '@/lib/seo'
import {
  useRetentionLastCleanup,
  useRetentionPolicy,
  useUpdateRetentionPolicy,
} from '@/hooks/use-retention'
import PageLayout from '@/components/page-layout'
import PageHeader from '@/components/page-header'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Checkbox } from '@/components/ui/checkbox'
import { ApiClientError, getApiErrorMessage } from '@/lib/api'
import {
  Form,
  FormControl,
  FormDescription,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
} from '@/components/ui/form'
import { Input } from '@/components/ui/input'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Skeleton } from '@/components/ui/skeleton'
import { Spinner } from '@/components/ui/spinner'

export const Route = createFileRoute('/settings/retention')({
  staticData: { breadcrumbLabel: 'Retention' },
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireAuthOrRedirect(instance.id)

    const user = useAuthStore.getState().user
    if (!user || (user.role !== 'owner' && user.role !== 'admin')) {
      throw redirect({ to: '/' })
    }
  },
  component: RetentionPage,
})

const TERMINAL_STATUSES = [
  { value: 'succeeded', label: 'Succeeded' },
  { value: 'failed', label: 'Failed' },
  { value: 'canceled', label: 'Canceled' },
  { value: 'timed_out', label: 'Timed Out' },
] as const

const CLEANUP_INTERVALS = [
  { value: '1800', label: '30 minutes' },
  { value: '3600', label: '1 hour' },
  { value: '21600', label: '6 hours' },
  { value: '86400', label: '24 hours' },
] as const

const retentionSchema = z.object({
  enabled: z.boolean(),
  max_age_days: z.string(),
  max_builds_per_project: z.string(),
  max_artifact_size_mb: z.string(),
  cleanup_target: z.enum(['artifacts_only', 'full']),
  keep_statuses: z.array(z.string()),
  dry_run: z.boolean(),
  cleanup_interval_secs: z.string(),
})

type RetentionFormValues = z.infer<typeof retentionSchema>

function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B'
  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  const i = Math.floor(Math.log(bytes) / Math.log(1024))
  return `${(bytes / Math.pow(1024, i)).toFixed(1)} ${units[i]}`
}

function formatRelativeTime(unixSecs: number): string {
  const now = Math.floor(Date.now() / 1000)
  const diff = now - unixSecs
  if (diff < 60) return 'just now'
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`
  return `${Math.floor(diff / 86400)}d ago`
}

function RetentionPage() {
  const { data: policyData, isLoading: policyLoading } = useRetentionPolicy()
  const { data: cleanupData, isLoading: cleanupLoading } =
    useRetentionLastCleanup()
  const updateMutation = useUpdateRetentionPolicy()

  const policy = policyData?.policy
  const lastCleanup = cleanupData?.last_cleanup

  const policyValues = policy
    ? {
        enabled: policy.enabled,
        max_age_days:
          policy.max_age_days != null ? String(policy.max_age_days) : '',
        max_builds_per_project:
          policy.max_builds_per_project != null
            ? String(policy.max_builds_per_project)
            : '',
        max_artifact_size_mb: policy.max_artifact_size_bytes
          ? String(Math.round(policy.max_artifact_size_bytes / (1024 * 1024)))
          : '',
        cleanup_target: policy.cleanup_target,
        keep_statuses: policy.keep_statuses,
        dry_run: policy.dry_run,
        cleanup_interval_secs: String(policy.cleanup_interval_secs),
      }
    : undefined

  const form = useForm<RetentionFormValues>({
    resolver: zodResolver(retentionSchema),
    defaultValues: {
      enabled: false,
      max_age_days: '',
      max_builds_per_project: '',
      max_artifact_size_mb: '',
      cleanup_target: 'artifacts_only',
      keep_statuses: [],
      dry_run: false,
      cleanup_interval_secs: '3600',
    },
    values: policyValues,
  })



  const enabled = form.watch('enabled')

  function onSubmit(values: RetentionFormValues) {
    const maxAgeDays =
      values.max_age_days.trim() === ''
        ? undefined
        : Number(values.max_age_days)
    const maxBuilds =
      values.max_builds_per_project.trim() === ''
        ? undefined
        : Number(values.max_builds_per_project)
    const maxSizeMb =
      values.max_artifact_size_mb.trim() === ''
        ? undefined
        : Number(values.max_artifact_size_mb)

    updateMutation.mutate(
      {
        enabled: values.enabled,
        max_age_days: maxAgeDays,
        max_builds_per_project: maxBuilds,
        max_artifact_size_bytes: maxSizeMb
          ? Math.round(maxSizeMb * 1024 * 1024)
          : undefined,
        cleanup_target: values.cleanup_target as RetentionCleanupTarget,
        keep_statuses: values.keep_statuses,
        dry_run: values.dry_run,
        cleanup_interval_secs: Number(values.cleanup_interval_secs),
      },
      {
        onSuccess: () => {
          toast.success('Retention policy updated')
        },
        onError: (error) => {
          const message =
            error instanceof ApiClientError
              ? getApiErrorMessage(error, {})
              : 'Failed to update retention policy'
          toast.error(message)
        },
      },
    )
  }

  if (policyLoading) {
    return (
      <PageLayout width="wide">
        <PageMeta title="Retention" />
        <PageHeader
          title="Retention Policy"
          description="Configure automatic cleanup of old builds and artifacts"
        />
        <Card>
          <CardContent>
            <div className="space-y-4 py-4">
              <Skeleton className="h-4 w-48" />
              <Skeleton className="h-10 w-full" />
              <Skeleton className="h-10 w-full" />
            </div>
          </CardContent>
        </Card>
      </PageLayout>
    )
  }

  return (
    <PageLayout width="wide">
      <PageMeta title="Retention" />
      <PageHeader
        title="Retention Policy"
        description="Configure automatic cleanup of old builds and artifacts to manage disk usage"
      />

      <Form {...form}>
        <form onSubmit={form.handleSubmit(onSubmit)} className="space-y-6">
          {/* Enable/Disable */}
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center justify-between">
                <span>Global Retention Policy</span>
                <Badge variant={enabled ? 'default' : 'outline'}>
                  {enabled ? 'Active' : 'Disabled'}
                </Badge>
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-6">
              <FormField
                control={form.control}
                name="enabled"
                render={({ field }) => (
                  <FormItem className="flex items-center gap-3">
                    <FormControl>
                      <Checkbox
                        checked={field.value}
                        onCheckedChange={field.onChange}
                      />
                    </FormControl>
                    <div className="space-y-0.5">
                      <FormLabel>Enable automatic cleanup</FormLabel>
                      <FormDescription>
                        When enabled, old builds and artifacts will be
                        automatically cleaned up based on the rules below
                      </FormDescription>
                    </div>
                  </FormItem>
                )}
              />

              {enabled && (
                <>
                  <div className="border-t pt-6">
                    <h4 className="text-sm font-medium mb-4">
                      Retention Criteria
                    </h4>
                    <p className="text-muted-foreground text-sm mb-4">
                      Builds matching any of the criteria below will be cleaned
                      up. Leave a field empty to disable that criterion.
                    </p>

                    <div className="grid gap-4 sm:grid-cols-3">
                      <FormField
                        control={form.control}
                        name="max_age_days"
                        render={({ field }) => (
                          <FormItem>
                            <FormLabel>Max age (days)</FormLabel>
                            <FormControl>
                              <Input
                                type="number"
                                min={1}
                                placeholder="e.g. 30"
                                {...field}
                              />
                            </FormControl>
                            <FormDescription>
                              Delete builds older than this
                            </FormDescription>
                            <FormMessage />
                          </FormItem>
                        )}
                      />

                      <FormField
                        control={form.control}
                        name="max_builds_per_project"
                        render={({ field }) => (
                          <FormItem>
                            <FormLabel>Max builds per project</FormLabel>
                            <FormControl>
                              <Input
                                type="number"
                                min={1}
                                placeholder="e.g. 100"
                                {...field}
                              />
                            </FormControl>
                            <FormDescription>
                              Keep only the N most recent
                            </FormDescription>
                            <FormMessage />
                          </FormItem>
                        )}
                      />

                      <FormField
                        control={form.control}
                        name="max_artifact_size_mb"
                        render={({ field }) => (
                          <FormItem>
                            <FormLabel>Max artifact size (MB)</FormLabel>
                            <FormControl>
                              <Input
                                type="number"
                                min={1}
                                placeholder="e.g. 5120"
                                {...field}
                              />
                            </FormControl>
                            <FormDescription>
                              Per-project artifact size cap
                            </FormDescription>
                            <FormMessage />
                          </FormItem>
                        )}
                      />
                    </div>
                  </div>

                  <div className="border-t pt-6">
                    <h4 className="text-sm font-medium mb-4">
                      Cleanup Behavior
                    </h4>

                    <div className="grid gap-4 sm:grid-cols-2">
                      <FormField
                        control={form.control}
                        name="cleanup_target"
                        render={({ field }) => (
                          <FormItem>
                            <FormLabel>Cleanup mode</FormLabel>
                            <Select
                              value={field.value}
                              onValueChange={field.onChange}
                            >
                              <FormControl>
                                <SelectTrigger>
                                  <SelectValue />
                                </SelectTrigger>
                              </FormControl>
                              <SelectContent>
                                <SelectItem value="artifacts_only">
                                  Artifacts only — keep build history
                                </SelectItem>
                                <SelectItem value="full">
                                  Full delete — remove everything
                                </SelectItem>
                              </SelectContent>
                            </Select>
                            <FormDescription>
                              &ldquo;Artifacts only&rdquo; deletes files but
                              preserves build logs and metadata
                            </FormDescription>
                            <FormMessage />
                          </FormItem>
                        )}
                      />

                      <FormField
                        control={form.control}
                        name="cleanup_interval_secs"
                        render={({ field }) => (
                          <FormItem>
                            <FormLabel>Cleanup interval</FormLabel>
                            <Select
                              value={field.value}
                              onValueChange={field.onChange}
                            >
                              <FormControl>
                                <SelectTrigger>
                                  <SelectValue />
                                </SelectTrigger>
                              </FormControl>
                              <SelectContent>
                                {CLEANUP_INTERVALS.map((interval) => (
                                  <SelectItem
                                    key={interval.value}
                                    value={interval.value}
                                  >
                                    {interval.label}
                                  </SelectItem>
                                ))}
                              </SelectContent>
                            </Select>
                            <FormDescription>
                              How often the cleanup job runs
                            </FormDescription>
                            <FormMessage />
                          </FormItem>
                        )}
                      />
                    </div>
                  </div>

                  <div className="border-t pt-6">
                    <h4 className="text-sm font-medium mb-4">
                      Protected Statuses
                    </h4>
                    <p className="text-muted-foreground text-sm mb-4">
                      Builds with these statuses will never be cleaned up,
                      regardless of other criteria.
                    </p>
                    <FormField
                      control={form.control}
                      name="keep_statuses"
                      render={({ field }) => (
                        <FormItem>
                          <div className="flex flex-wrap gap-4">
                            {TERMINAL_STATUSES.map((status) => (
                              <label
                                key={status.value}
                                className="flex items-center gap-2 text-sm"
                              >
                                <Checkbox
                                  checked={field.value.includes(status.value)}
                                  onCheckedChange={(checked) => {
                                    if (checked) {
                                      field.onChange([
                                        ...field.value,
                                        status.value,
                                      ])
                                    } else {
                                      field.onChange(
                                        field.value.filter(
                                          (s) => s !== status.value,
                                        ),
                                      )
                                    }
                                  }}
                                />
                                {status.label}
                              </label>
                            ))}
                          </div>
                          <FormMessage />
                        </FormItem>
                      )}
                    />
                  </div>

                  <div className="border-t pt-6">
                    <FormField
                      control={form.control}
                      name="dry_run"
                      render={({ field }) => (
                        <FormItem className="flex items-center gap-3">
                          <FormControl>
                            <Checkbox
                              checked={field.value}
                              onCheckedChange={field.onChange}
                            />
                          </FormControl>
                          <div className="space-y-0.5">
                            <FormLabel>Dry run mode</FormLabel>
                            <FormDescription>
                              When enabled, the cleanup job will log what it
                              would delete without actually removing anything.
                              Useful for testing your policy.
                            </FormDescription>
                          </div>
                        </FormItem>
                      )}
                    />
                  </div>
                </>
              )}

              <div className="flex justify-end pt-2">
                <Button type="submit" disabled={updateMutation.isPending}>
                  {updateMutation.isPending && <Spinner className="mr-2" />}
                  Save Policy
                </Button>
              </div>
            </CardContent>
          </Card>
        </form>
      </Form>

      {/* Last Cleanup Summary */}
      <Card>
        <CardHeader>
          <CardTitle>Last Cleanup</CardTitle>
        </CardHeader>
        <CardContent>
          {cleanupLoading ? (
            <div className="space-y-2">
              <Skeleton className="h-4 w-64" />
              <Skeleton className="h-4 w-48" />
            </div>
          ) : lastCleanup ? (
            <div className="grid gap-4 sm:grid-cols-4">
              <div>
                <p className="text-muted-foreground text-sm">Builds cleaned</p>
                <p className="text-lg font-semibold">
                  {lastCleanup.builds_expired}
                </p>
              </div>
              <div>
                <p className="text-muted-foreground text-sm">
                  Artifacts deleted
                </p>
                <p className="text-lg font-semibold">
                  {lastCleanup.artifacts_deleted}
                </p>
              </div>
              <div>
                <p className="text-muted-foreground text-sm">Space reclaimed</p>
                <p className="text-lg font-semibold">
                  {formatBytes(lastCleanup.bytes_reclaimed)}
                </p>
              </div>
              <div>
                <p className="text-muted-foreground text-sm">Ran</p>
                <p className="text-lg font-semibold">
                  {formatRelativeTime(lastCleanup.ran_at)}
                  {lastCleanup.dry_run && (
                    <Badge variant="outline" className="ml-2">
                      Dry run
                    </Badge>
                  )}
                </p>
              </div>
            </div>
          ) : (
            <p className="text-muted-foreground text-sm">
              No cleanup has run yet. Enable the retention policy and wait for
              the next scheduled run.
            </p>
          )}
        </CardContent>
      </Card>
    </PageLayout>
  )
}
