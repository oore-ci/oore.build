import { Link } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import { Alert02Icon } from '@hugeicons/core-free-icons'

import { useInstancePreferences } from '@/hooks/use-artifact-storage'
import { useRunnerPolicyRepositories } from '@/hooks/use-source-repositories'
import { useAuthStore } from '@/stores/auth-store'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'

export interface DirectRunnerPolicySummary {
  instanceEnabled: boolean
  repositoriesKnown: boolean
  unapprovedRepositoryCount: number
}

export function needsDirectRunnerPolicySetup(
  summary: DirectRunnerPolicySummary,
): boolean {
  return (
    !summary.instanceEnabled ||
    !summary.repositoriesKnown ||
    summary.unapprovedRepositoryCount > 0
  )
}

export default function DirectRunnerPolicyBanner() {
  const role = useAuthStore((state) => state.user?.role)
  const canConfigure = role === 'owner' || role === 'admin'
  const preferencesQuery = useInstancePreferences({ enabled: canConfigure })
  const repositoriesQuery = useRunnerPolicyRepositories(canConfigure)

  if (!canConfigure) return null
  if (preferencesQuery.isLoading || repositoriesQuery.isLoading) return null

  const preferences = preferencesQuery.data?.preferences
  const repositories = repositoriesQuery.data
  const summary: DirectRunnerPolicySummary = {
    instanceEnabled: preferences?.direct_macos_runner_enabled ?? false,
    repositoriesKnown:
      !preferencesQuery.isError && !repositoriesQuery.isError && !!repositories,
    unapprovedRepositoryCount:
      repositories?.filter(
        (repository) => !repository.allow_direct_macos_runner,
      ).length ?? 0,
  }

  if (!needsDirectRunnerPolicySetup(summary)) return null

  return (
    <Alert className="rounded-none border-x-0 border-t-0 border-warning/30 bg-warning/10 text-foreground">
      <HugeiconsIcon
        icon={Alert02Icon}
        size={16}
        className="text-warning"
        aria-hidden
      />
      <AlertTitle>Direct runner setup required</AlertTitle>
      <AlertDescription className="flex flex-wrap items-center gap-x-3 gap-y-1">
        <span>
          Build code now runs with the runner account&apos;s macOS permissions.
          Enable the instance runner, then approve each trusted repository.
        </span>
        <span className="ml-auto flex items-center gap-3">
          <Link
            to="/settings/preferences"
            className="inline-flex min-h-11 items-center font-medium sm:min-h-0"
          >
            Preferences
          </Link>
          <Link
            to="/settings/integrations"
            className="inline-flex min-h-11 items-center font-medium sm:min-h-0"
          >
            Sources
          </Link>
        </span>
      </AlertDescription>
    </Alert>
  )
}
