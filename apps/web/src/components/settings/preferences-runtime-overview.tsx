import { toast } from '@/lib/toast'
import { HugeiconsIcon } from '@hugeicons/react'
import { Download04Icon } from '@hugeicons/core-free-icons'
import type { useRuntimeUpdates } from '@/hooks/use-runtime-updates'
import type { RuntimeUpdateStatus } from '@oore/client/models'
import { runtimeUpdateActive } from '@/components/settings/preferences-utils'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import {
  Item,
  ItemActions,
  ItemContent,
  ItemDescription,
  ItemGroup,
  ItemSeparator,
  ItemTitle,
} from '@/components/ui/item'
import {
  RuntimeUpdatePreflightChecks,
  backendUpdateReady,
} from '@/components/runtime-update-preflight'
import {
  SettingsSection,
  SettingsSurface,
} from '@/components/settings/settings-section'

export function RuntimeOverview({
  backendUpdatePhase,
  backendVersionLabel,
  frontendUpdatePhase,
  isOwner,
  runtimeUpdates,
  webVersionLabel,
}: {
  backendUpdatePhase: RuntimeUpdateStatus['phase'] | undefined
  backendVersionLabel: string
  frontendUpdatePhase: RuntimeUpdateStatus['phase'] | undefined
  isOwner: boolean
  runtimeUpdates: ReturnType<typeof useRuntimeUpdates>
  webVersionLabel: string
}) {
  const webHealthQuery = runtimeUpdates.frontendHealth
  const backendHealthQuery = runtimeUpdates.backendHealth
  const backendReady = backendUpdateReady(
    runtimeUpdates.backendPreflight,
    runtimeUpdates.backendUpdate.data?.managed_service,
  )
  const backendUpdateFailure =
    runtimeUpdates.backendUpdate.data?.phase === 'failed'
      ? runtimeUpdates.backendUpdate.data.error
      : null
  const frontendRelease = runtimeUpdates.frontendRelease.data

  return (
    <SettingsSection
      title="Runtime"
      description="Installed frontend and backend versions for this instance."
    >
      <SettingsSurface inset={false}>
        <ItemGroup aria-label="Runtime versions" className="gap-0">
          <Item>
            <ItemContent>
              <ItemTitle>Frontend</ItemTitle>
              <ItemDescription className="line-clamp-none">
                <span className="font-mono text-foreground">
                  {webVersionLabel}
                </span>
                {' · '}
                {webHealthQuery.data?.channel
                  ? `${webHealthQuery.data.channel} channel`
                  : 'Loaded oore-web bundle'}
              </ItemDescription>
              {runtimeUpdates.frontendOffered &&
              runtimeUpdates.frontendRelease.data?.update_available ? (
                <>
                  <ItemDescription className="line-clamp-none text-foreground">
                    {runtimeUpdates.frontendRelease.data.latest_version} is
                    available
                  </ItemDescription>
                  {!runtimeUpdates.frontendRelease.data.managed_service ? (
                    <ItemDescription className="line-clamp-none">
                      {runtimeUpdates.frontendRelease.data
                        .managed_service_reason ||
                        'Install oore-web as a managed service to update it here.'}
                    </ItemDescription>
                  ) : null}
                </>
              ) : null}
              {isOwner && runtimeUpdates.frontendRelease.isError ? (
                <ItemDescription className="line-clamp-none">
                  Could not check for frontend updates:{' '}
                  {runtimeUpdates.frontendRelease.error.message}
                </ItemDescription>
              ) : null}
            </ItemContent>
            {runtimeUpdates.frontendOffered &&
            runtimeUpdates.frontendRelease.data?.update_available ? (
              <ItemActions>
                <Button
                  size="sm"
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
                  <HugeiconsIcon
                    icon={Download04Icon}
                    data-icon="inline-start"
                  />
                  {runtimeUpdates.startFrontendUpdate.isPending
                    ? 'Starting...'
                    : frontendUpdatePhase === 'restarting'
                      ? 'Restarting...'
                      : frontendUpdatePhase === 'updating'
                        ? 'Updating...'
                        : 'Update frontend'}
                </Button>
              </ItemActions>
            ) : null}
          </Item>

          <ItemSeparator className="my-0" />

          <Item>
            <ItemContent>
              <ItemTitle>Backend</ItemTitle>
              <ItemDescription className="line-clamp-none">
                <span className="font-mono text-foreground">
                  {backendVersionLabel}
                </span>
                {' · '}
                {backendHealthQuery.data?.channel
                  ? `${backendHealthQuery.data.channel} channel`
                  : 'Loaded oored daemon'}
              </ItemDescription>
              {runtimeUpdates.backendOffered &&
              runtimeUpdates.backendRelease.data?.update_available ? (
                <>
                  <ItemDescription className="line-clamp-none text-foreground">
                    {runtimeUpdates.backendRelease.data.latest_version} is
                    available
                  </ItemDescription>
                </>
              ) : null}
              {isOwner ? (
                <RuntimeUpdatePreflightChecks
                  preflight={runtimeUpdates.backendPreflight}
                />
              ) : null}
            </ItemContent>
            {runtimeUpdates.backendOffered &&
            runtimeUpdates.backendRelease.data?.update_available ? (
              <ItemActions>
                <Button
                  size="sm"
                  disabled={
                    !isOwner ||
                    !backendReady ||
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
                  <HugeiconsIcon
                    icon={Download04Icon}
                    data-icon="inline-start"
                  />
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
              </ItemActions>
            ) : null}
          </Item>
        </ItemGroup>
      </SettingsSurface>

      {frontendRelease?.phase === 'failed' ? (
        <Alert variant="destructive">
          <AlertTitle>Frontend update failed</AlertTitle>
          <AlertDescription className="flex flex-col gap-2 wrap-break-word">
            <p>
              {frontendRelease.error ||
                'The frontend update stopped before completion.'}
            </p>
            {frontendRelease.log_path ? (
              <p>
                Check{' '}
                <code className="font-mono text-[11px]">
                  {frontendRelease.log_path}
                </code>{' '}
                on the web node for details, then retry.
              </p>
            ) : null}
          </AlertDescription>
        </Alert>
      ) : null}

      {runtimeUpdates.backendUpdate.data?.phase === 'failed' ? (
        <Alert variant="destructive">
          <AlertTitle>Backend update failed</AlertTitle>
          <AlertDescription className="flex flex-col gap-2 wrap-break-word">
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
    </SettingsSection>
  )
}
