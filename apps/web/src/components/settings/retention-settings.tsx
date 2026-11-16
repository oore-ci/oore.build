import { useForm, useWatch } from 'react-hook-form'
import type { UseFormReturn } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import * as z from 'zod'
import { toast } from '@/lib/toast'

import {
  useRetentionLastCleanup,
  useRetentionPolicy,
  useUpdateRetentionPolicy,
} from '@/hooks/use-retention'
import { FormActions } from '@/components/form-actions'
import {
  SettingsSection,
  SettingsSurface,
  SettingsSurfaceSeparator,
} from '@/components/settings/settings-section'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import { isOoreApiError } from '@oore/client/client'
import { getApiErrorMessage } from '@/lib/api-client/api-error'
import {
  Form,
  FormControl,
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
import { Switch } from '@/components/ui/switch'
import { RetentionSummaryCard } from '@/components/settings/retention-summary-card'
import { Stack } from '@/components/layout/stack'
import { splitRowVariants } from '@/lib/recipes'

const TERMINAL_STATUSES = [
  { value: 'succeeded', label: 'Succeeded' },
  { value: 'failed', label: 'Failed' },
  { value: 'canceled', label: 'Canceled' },
  { value: 'timed_out', label: 'Timed out' },
] as const

const CLEANUP_INTERVALS = [
  { value: '1800', label: '30 minutes' },
  { value: '3600', label: '1 hour' },
  { value: '21600', label: '6 hours' },
  { value: '86400', label: '24 hours' },
] as const

const CLEANUP_TARGETS = {
  artifacts_only: 'Artifacts only, keep build history',
  full: 'Full delete, remove everything',
} as const

const CLEANUP_INTERVAL_OPTIONS = Object.fromEntries(
  CLEANUP_INTERVALS.map(({ value, label }) => [value, label]),
)

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

function EnabledRetentionFields({
  form,
}: {
  form: UseFormReturn<RetentionFormValues>
}) {
  return (
    <>
      <SettingsSection
        title="Retention criteria"
        hint="A build is cleaned up when it matches any limit. Leave a field empty to disable that limit."
      >
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
                <FormMessage />
              </FormItem>
            )}
          />

          <FormField
            control={form.control}
            name="max_builds_per_project"
            render={({ field }) => (
              <FormItem>
                <FormLabel>Max builds per app</FormLabel>
                <FormControl>
                  <Input
                    type="number"
                    min={1}
                    placeholder="e.g. 100"
                    {...field}
                  />
                </FormControl>
                <FormMessage />
              </FormItem>
            )}
          />

          <FormField
            control={form.control}
            name="max_artifact_size_mb"
            render={({ field }) => (
              <FormItem>
                <FormLabel>Max artifact storage per app (MB)</FormLabel>
                <FormControl>
                  <Input
                    type="number"
                    min={1}
                    placeholder="e.g. 5120"
                    {...field}
                  />
                </FormControl>
                <FormMessage />
              </FormItem>
            )}
          />
        </div>
      </SettingsSection>

      <SettingsSurfaceSeparator />

      <SettingsSection title="Cleanup behavior">
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
                  items={CLEANUP_TARGETS}
                >
                  <FormControl>
                    <SelectTrigger className="w-full">
                      <SelectValue />
                    </SelectTrigger>
                  </FormControl>
                  <SelectContent>
                    <SelectItem value="artifacts_only">
                      Artifacts only, keep build history
                    </SelectItem>
                    <SelectItem value="full">
                      Full delete, remove everything
                    </SelectItem>
                  </SelectContent>
                </Select>
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
                  items={CLEANUP_INTERVAL_OPTIONS}
                >
                  <FormControl>
                    <SelectTrigger className="w-full">
                      <SelectValue />
                    </SelectTrigger>
                  </FormControl>
                  <SelectContent>
                    {CLEANUP_INTERVALS.map((interval) => (
                      <SelectItem key={interval.value} value={interval.value}>
                        {interval.label}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
                <FormMessage />
              </FormItem>
            )}
          />
        </div>
      </SettingsSection>

      <SettingsSurfaceSeparator />

      <SettingsSection
        title="Protected statuses"
        hint="Builds with these statuses are never cleaned up, whatever the other criteria say."
      >
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
                          field.onChange([...field.value, status.value])
                        } else {
                          field.onChange(
                            field.value.filter((s) => s !== status.value),
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
      </SettingsSection>

      <SettingsSurfaceSeparator />

      <SettingsSection
        title="Dry run"
        hint="Log what the policy would remove without deleting anything."
        actions={
          <FormField
            control={form.control}
            name="dry_run"
            render={({ field }) => (
              <FormItem>
                <FormControl>
                  <Switch
                    checked={field.value}
                    onCheckedChange={field.onChange}
                    aria-label="Enable dry run"
                  />
                </FormControl>
              </FormItem>
            )}
          />
        }
      />
    </>
  )
}

/**
 * The Retention section of the Storage settings page: when Oore cleans up
 * old builds, logs and artifacts.
 */
export function RetentionSettings() {
  const {
    data: policyData,
    error: policyError,
    isLoading: policyLoading,
    refetch: refetchPolicy,
  } = useRetentionPolicy()
  const {
    data: cleanupData,
    error: cleanupError,
    isLoading: cleanupLoading,
    refetch: refetchCleanup,
  } = useRetentionLastCleanup()
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

  const enabled = useWatch({ control: form.control, name: 'enabled' })

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
        cleanup_target: values.cleanup_target,
        keep_statuses: values.keep_statuses,
        dry_run: values.dry_run,
        cleanup_interval_secs: Number(values.cleanup_interval_secs),
        // The daemon replaces the whole policy, so keep settings this form
        // does not show.
        artifact_ttl_days: policy?.artifact_ttl_days,
      },
      {
        onSuccess: () => {
          toast.success('Retention policy updated')
          form.reset(values)
        },
        onError: (error) => {
          const message = isOoreApiError(error)
            ? getApiErrorMessage(error, {})
            : 'Failed to update retention policy'
          toast.error(message)
        },
      },
    )
  }

  if (policyLoading) {
    return (
      <SettingsSection id="retention" title="Retention">
        <Stack gap={4} aria-label="Loading retention settings">
          <Skeleton className="h-4 w-48" />
          <Skeleton className="h-10 w-full" />
          <Skeleton className="h-10 w-full" />
        </Stack>
      </SettingsSection>
    )
  }

  if (policyError || !policy) {
    const message =
      policyError?.message ?? 'The response did not include a retention policy.'
    return (
      <SettingsSection id="retention" title="Retention">
        <Alert variant="destructive">
          <AlertDescription className={splitRowVariants()}>
            <span>Failed to load the retention policy: {message}</span>
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={() => void refetchPolicy()}
            >
              Retry
            </Button>
          </AlertDescription>
        </Alert>
      </SettingsSection>
    )
  }

  return (
    <SettingsSection
      id="retention"
      title="Retention"
      meta={
        <RetentionSummaryCard
          error={cleanupError}
          isLoading={cleanupLoading}
          lastCleanup={lastCleanup ?? undefined}
          onRetry={() => void refetchCleanup()}
        />
      }
    >
      <Form {...form}>
        <form onSubmit={form.handleSubmit(onSubmit)}>
          <SettingsSurface className="flex flex-col gap-6">
            <FormField
              control={form.control}
              name="enabled"
              render={({ field }) => (
                <SettingsSection
                  title="Automatic cleanup"
                  hint="Running builds and protected statuses are never removed."
                  actions={
                    <FormItem>
                      <FormControl>
                        <Switch
                          checked={field.value}
                          onCheckedChange={field.onChange}
                          aria-label="Enable automatic cleanup"
                        />
                      </FormControl>
                    </FormItem>
                  }
                />
              )}
            />

            {enabled ? (
              <>
                <SettingsSurfaceSeparator />
                <EnabledRetentionFields form={form} />
              </>
            ) : null}
          </SettingsSurface>
          <FormActions
            className="mt-4"
            disabled={!form.formState.isDirty}
            disabledReason="No changes to save"
            isPending={updateMutation.isPending}
            submitLabel="Save changes"
          />
        </form>
      </Form>
    </SettingsSection>
  )
}
