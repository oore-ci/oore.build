import { toast } from '@/lib/toast'
import { HugeiconsIcon } from '@hugeicons/react'
import { Download04Icon } from '@hugeicons/core-free-icons'
import type { PreferencesPageState } from '@/routes/settings/preferences'
import { runtimeUpdateActive } from '@/components/settings/preferences-utils'
import { installerCommand } from '@/components/runtime-update-utils'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
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
  const backendUpdateFailure =
    runtimeUpdates.backendUpdate.data?.phase === 'failed'
      ? runtimeUpdates.backendUpdate.data.error
      : null

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
                      : backendUpdatePhase === 'failed'
                        ? 'Retry backend update'
                        : 'Update backend'}
              </Button>
              {runtimeUpdates.backendUpdate.data &&
              !runtimeUpdates.backendUpdate.data.managed_service ? (
                <div className="mt-2 space-y-2 text-xs text-muted-foreground">
                  <p>
                    Run the current installer once from Terminal to finish or
                    repair managed service setup. Later backend updates remain
                    available here.
                  </p>
                  <code className="block break-all rounded-md bg-muted p-2 font-mono text-foreground">
                    {installerCommand(
                      runtimeUpdates.backendRelease.data.channel,
                    )}
                  </code>
                </div>
              ) : null}
            </>
          ) : null}
          {runtimeUpdates.backendUpdate.data?.phase === 'failed' ? (
            <Alert variant="destructive" className="mt-3">
              <AlertTitle>Backend update failed</AlertTitle>
              <AlertDescription className="space-y-2 break-words">
                <p>
                  {backendUpdateFailure ||
                    'The supervised backend update stopped before completion.'}
                </p>
                <p>
                  Check{' '}
                  <code className="font-mono text-[11px]">
                    &lt;install root&gt;/logs/update-supervisor.log
                  </code>{' '}
                  on the backend Mac for the rollback details, then retry.
                </p>
              </AlertDescription>
            </Alert>
          ) : null}
        </CardContent>
      </Card>
    </section>
  )
}
