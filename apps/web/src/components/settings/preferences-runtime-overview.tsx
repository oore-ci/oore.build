import { toast } from 'sonner'
import { HugeiconsIcon } from '@hugeicons/react'
import { Download04Icon } from '@hugeicons/core-free-icons'
import type { PreferencesPageState } from '@/routes/settings/preferences'
import { runtimeUpdateActive } from '@/components/settings/preferences-utils'
import { Button } from '@/components/ui/button'
import { Card, CardContent } from '@/components/ui/card'

export function RuntimeOverview({ state }: { state: PreferencesPageState }) {
  const {
    artifactBackendLabel,
    artifactSourceLabel,
    backendHealthQuery,
    backendUpdatePhase,
    backendVersionLabel,
    frontendUpdatePhase,
    isOwner,
    runtimeUpdates,
    webHealthQuery,
    webVersionLabel,
  } = state
  return (
    <section className="grid gap-4 md:grid-cols-2 xl:grid-cols-4">
      <Card>
        <CardContent>
          <p className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
            Artifact backend
          </p>
          <p className="mt-3 text-2xl font-bold tracking-tight">
            {artifactBackendLabel}
          </p>
          <p className="mt-1 text-xs text-muted-foreground">
            Where build artifacts are stored
          </p>
        </CardContent>
      </Card>
      <Card>
        <CardContent>
          <p className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
            Config source
          </p>
          <p className="mt-3 text-2xl font-bold tracking-tight">
            {artifactSourceLabel}
          </p>
          <p className="mt-1 text-xs text-muted-foreground">
            Effective settings source
          </p>
        </CardContent>
      </Card>
      <Card>
        <CardContent className="flex h-full flex-col">
          <p className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
            Frontend version
          </p>
          <p className="mt-3 font-mono text-2xl font-bold tracking-tight">
            {webVersionLabel}
          </p>
          <p className="mt-1 text-xs text-muted-foreground">
            {webHealthQuery.data?.channel
              ? `${webHealthQuery.data.channel} channel`
              : 'Loaded oore-web bundle'}
          </p>
          {runtimeUpdates.frontendRelease.data?.update_available ? (
            <>
              <p className="mt-2 text-xs text-primary">
                {runtimeUpdates.frontendRelease.data.latest_version} is
                available
              </p>
              <Button
                size="sm"
                className="mt-3 self-start"
                disabled={
                  !isOwner ||
                  !runtimeUpdates.frontendRelease.data.managed_service ||
                  runtimeUpdateActive(frontendUpdatePhase) ||
                  runtimeUpdates.startFrontendUpdate.isPending
                }
                onClick={() =>
                  runtimeUpdates.startFrontendUpdate.mutate(undefined, {
                    onSuccess: () =>
                      toast.success(
                        'Frontend update started. The UI will reconnect after restart.',
                      ),
                    onError: (error) => toast.error(error.message),
                  })
                }
              >
                <HugeiconsIcon icon={Download04Icon} />
                {runtimeUpdates.startFrontendUpdate.isPending
                  ? 'Starting...'
                  : frontendUpdatePhase === 'restarting'
                    ? 'Restarting...'
                    : frontendUpdatePhase === 'updating'
                      ? 'Updating...'
                      : 'Update frontend'}
              </Button>
              {!runtimeUpdates.frontendRelease.data.managed_service ? (
                <p className="mt-2 text-xs text-muted-foreground">
                  Install oore-web as a managed service to update it here.
                </p>
              ) : null}
            </>
          ) : null}
        </CardContent>
      </Card>
      <Card>
        <CardContent className="flex h-full flex-col">
          <p className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
            Backend version
          </p>
          <p className="mt-3 font-mono text-2xl font-bold tracking-tight">
            {backendVersionLabel}
          </p>
          <p className="mt-1 text-xs text-muted-foreground">
            {backendHealthQuery.data?.channel
              ? `${backendHealthQuery.data.channel} channel`
              : 'Loaded oored daemon'}
          </p>
          {runtimeUpdates.backendRelease.data?.update_available ? (
            <>
              <p className="mt-2 text-xs text-primary">
                {runtimeUpdates.backendRelease.data.latest_version} is available
              </p>
              <Button
                size="sm"
                className="mt-3 self-start"
                disabled={
                  !isOwner ||
                  !runtimeUpdates.backendUpdate.data?.managed_service ||
                  runtimeUpdateActive(backendUpdatePhase) ||
                  runtimeUpdates.startBackendUpdate.isPending
                }
                onClick={() =>
                  runtimeUpdates.startBackendUpdate.mutate(undefined, {
                    onSuccess: () =>
                      toast.success(
                        'Backend update started. Readiness will recover after launchd restarts it.',
                      ),
                    onError: (error) => toast.error(error.message),
                  })
                }
              >
                <HugeiconsIcon icon={Download04Icon} />
                {runtimeUpdates.startBackendUpdate.isPending
                  ? 'Starting...'
                  : backendUpdatePhase === 'restarting'
                    ? 'Restarting...'
                    : backendUpdatePhase === 'updating'
                      ? 'Updating...'
                      : 'Update backend'}
              </Button>
              {runtimeUpdates.backendUpdate.data &&
              !runtimeUpdates.backendUpdate.data.managed_service ? (
                <p className="mt-2 text-xs text-muted-foreground">
                  Install oored as the managed macOS service to update it here.
                </p>
              ) : null}
            </>
          ) : null}
        </CardContent>
      </Card>
    </section>
  )
}
