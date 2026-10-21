import { HugeiconsIcon } from '@hugeicons/react'
import { Alert02Icon } from '@hugeicons/core-free-icons'

import {
  useInstancePreferences,
  useUpdateInstancePreferences,
} from '@/hooks/use-artifact-storage'
import { useHasPermission } from '@/hooks/use-permissions'
import { toast } from '@/lib/toast'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import {
  Item,
  ItemActions,
  ItemContent,
  ItemDescription,
  ItemTitle,
} from '@/components/ui/item'
import { Skeleton } from '@/components/ui/skeleton'
import { Switch } from '@/components/ui/switch'
import {
  SettingsSection,
  SettingsSurface,
} from '@/components/settings/settings-section'

export function DirectRunnerPolicyPanel() {
  const canRead = useHasPermission('instance_settings:read')

  if (!canRead) return null

  return (
    <SettingsSection
      title="Direct macOS runner"
      description="The runner built into this Mac. Pausing it lets assigned and running builds finish and stops new claims."
    >
      <DirectRunnerPolicyControl />
    </SettingsSection>
  )
}

function DirectRunnerPolicyControl() {
  const preferencesQuery = useInstancePreferences()
  const mutation = useUpdateInstancePreferences()
  const canWrite = useHasPermission('instance_settings:write')
  const preferences = preferencesQuery.data
  const enabled = !(preferences?.direct_macos_runner_paused ?? false)

  if (preferencesQuery.isLoading) {
    return <Skeleton className="h-16 w-full" />
  }

  if (preferencesQuery.error) {
    return (
      <Alert variant="destructive">
        <HugeiconsIcon icon={Alert02Icon} aria-hidden />
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
    if (!currentPreferences || !canWrite || mutation.isPending) return

    mutation.mutate(
      {
        key_storage_mode: currentPreferences.key_storage_mode,
        direct_macos_runner_paused: !checked,
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
    <SettingsSurface inset={false}>
      <Item>
        <ItemContent>
          <ItemTitle>Accept builds from approved repositories</ItemTitle>
          <ItemDescription>
            {enabled
              ? 'New builds from approved repositories run on this Mac.'
              : 'Paused. Builds in progress finish; new builds wait for another runner.'}
          </ItemDescription>
          {!canWrite ? (
            <ItemDescription>
              Only an owner or admin can change this.
            </ItemDescription>
          ) : null}
        </ItemContent>
        <ItemActions className="ml-auto">
          <Switch
            id="direct-runner-policy"
            aria-label="Accept builds from approved repositories"
            checked={enabled}
            disabled={!canWrite || mutation.isPending}
            onCheckedChange={updatePolicy}
          />
        </ItemActions>
      </Item>
    </SettingsSurface>
  )
}
