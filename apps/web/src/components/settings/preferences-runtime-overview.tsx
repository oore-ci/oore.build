import { toast } from '@/lib/toast'
import { DynamicLucideIcon } from '@/components/ui/dynamic-lucide-icon'
import { Download as Download04Icon } from 'lucide-react'
import type { PreferencesPageState } from '@/routes/settings/preferences'
import { runtimeUpdateActive } from '@/components/settings/preferences-utils'
import { installerCommand } from '@/components/runtime-update-utils'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'

export function RuntimeOverview({ state }: { state: PreferencesPageState }) {
  const {
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
    <section
      aria-label="Runtime versions"
      className="grid border md:grid-cols-2 md:divide-x"
    >
      <div className="flex min-w-0 flex-col p-4">
        <p className="text-sm font-medium">Frontend</p>
        <p className="mt-2 font-mono text-lg font-semibold tracking-tight">
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
              {runtimeUpdates.frontendRelease.data.latest_version} is available
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
              <DynamicLucideIcon icon={Download04Icon} />
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
      </div>
      <div className="flex min-w-0 flex-col border-t p-4 md:border-t-0">
        <p className="text-sm font-medium">Backend</p>
        <p className="mt-2 font-mono text-lg font-semibold tracking-tight">
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
              <DynamicLucideIcon icon={Download04Icon} />
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
                <code className="block rounded-md bg-muted p-2 font-mono break-all text-foreground">
                  {installerCommand(runtimeUpdates.backendRelease.data.channel)}
                </code>
              </div>
            ) : null}
          </>
        ) : null}
        {runtimeUpdates.backendUpdate.data?.phase === 'failed' ? (
          <Alert variant="destructive" className="mt-3">
            <AlertTitle>Backend update failed</AlertTitle>
            <AlertDescription className="space-y-2 wrap-break-word">
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
      </div>
    </section>
  )
}
