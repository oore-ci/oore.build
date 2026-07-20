import { TriangleAlert as Alert02Icon } from 'lucide-react'
import { DynamicLucideIcon } from '@/components/ui/dynamic-lucide-icon'

import {
  useInstancePreferences,
  useUpdateInstancePreferences,
} from '@/hooks/use-artifact-storage'
import { useHasPermission } from '@/hooks/use-permissions'
import { toast } from '@/lib/toast'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Skeleton } from '@/components/ui/skeleton'
import { Switch } from '@/components/ui/switch'

export function DirectRunnerPolicyPanel() {
  const canRead = useHasPermission('instance_settings', 'read')

  if (!canRead) return null

  return <DirectRunnerPolicyControl />
}

function DirectRunnerPolicyControl() {
  const preferencesQuery = useInstancePreferences()
  const updatePreferences = useUpdateInstancePreferences()
  const canWrite = useHasPermission('instance_settings', 'write')
  const preferences = preferencesQuery.data?.preferences
  const enabled = preferences?.direct_macos_runner_enabled ?? false

  if (preferencesQuery.isLoading) {
    return (
      <section
        className="space-y-3 border bg-card p-4"
        aria-label="Direct runner policy"
      >
        <Skeleton className="h-4 w-40" />
        <Skeleton className="h-9 w-full" />
      </section>
    )
  }

  if (preferencesQuery.error) {
    return (
      <Alert variant="destructive">
        <DynamicLucideIcon icon={Alert02Icon} aria-hidden />
        <AlertDescription className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
          <span>
            Failed to load Direct runner policy:{' '}
            {preferencesQuery.error.message}
          </span>
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={() => void preferencesQuery.refetch()}
          >
            Retry
          </Button>
        </AlertDescription>
      </Alert>
    )
  }

  if (!preferences) return null

  function updatePolicy(checked: boolean) {
    const currentPreferences = preferences
    if (!currentPreferences || !canWrite || updatePreferences.isPending) return

    updatePreferences.mutate(
      {
        key_storage_mode: currentPreferences.key_storage_mode,
        direct_macos_runner_enabled: checked,
      },
      {
        onSuccess: () =>
          toast.success(
            checked
              ? 'Direct macOS runner enabled.'
              : 'Direct macOS runner paused. Running builds will finish.',
          ),
        onError: (error) =>
          toast.error(`Failed to update runner policy: ${error.message}`),
      },
    )
  }

  return (
    <section
      className="border bg-card"
      aria-labelledby="direct-runner-policy-title"
    >
      <div className="flex flex-wrap items-center justify-between gap-3 border-b px-4 py-3">
        <div>
          <h2 id="direct-runner-policy-title" className="text-sm font-semibold">
            Direct runner policy
          </h2>
          <p className="mt-0.5 text-xs text-muted-foreground">
            Global execution gate for repositories approved under Sources.
          </p>
        </div>
        <Badge variant={enabled ? 'default' : 'outline'}>
          {enabled ? 'Enabled' : 'Paused'}
        </Badge>
      </div>

      <div className="flex flex-col gap-4 p-4 sm:flex-row sm:items-start sm:justify-between">
        <div className="max-w-3xl space-y-2">
          <p className="text-sm text-foreground">
            Repository commands run with the macOS permissions of the runner
            account. Enable only repositories you would run directly on this
            Mac.
          </p>
          <p className="text-xs text-muted-foreground">
            Pausing prevents new jobs from being claimed. Assigned and running
            jobs continue to completion.
          </p>
          {!canWrite ? (
            <p className="text-xs text-muted-foreground">
              You have read-only access to this instance policy.
            </p>
          ) : null}
        </div>
        <div className="flex min-h-11 shrink-0 items-center gap-3">
          <label htmlFor="direct-runner-policy" className="text-sm font-medium">
            Allow approved repositories
          </label>
          <Switch
            id="direct-runner-policy"
            checked={enabled}
            disabled={!canWrite || updatePreferences.isPending}
            onCheckedChange={updatePolicy}
          />
        </div>
      </div>
    </section>
  )
}
