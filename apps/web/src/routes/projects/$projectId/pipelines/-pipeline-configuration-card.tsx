import { useState } from 'react'
import type { ReactNode } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import { ArrowDown01Icon, ArrowRight01Icon } from '@hugeicons/core-free-icons'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Skeleton } from '@/components/ui/skeleton'
import { RelativeTime } from '@/components/relative-time'
import { formatEnumLabel } from '@/lib/format-utils'
import { Card, CardContent } from '@/components/ui/card'
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible'
import type {
  Pipeline,
  PipelineAndroidSigningResponse,
  PipelineIosSigningResponse,
} from '@oore/client/models'
import { Text } from '@/components/layout/text'

function Section({
  title,
  defaultOpen = false,
  children,
}: {
  title: string
  defaultOpen?: boolean
  children: ReactNode
}) {
  const [open, setOpen] = useState(defaultOpen)
  const SectionIcon = open ? ArrowDown01Icon : ArrowRight01Icon

  return (
    <Collapsible open={open} onOpenChange={setOpen}>
      <CollapsibleTrigger className="flex w-full items-center gap-2 py-2 text-sm font-medium">
        <HugeiconsIcon icon={SectionIcon} size={14} />
        {title}
      </CollapsibleTrigger>
      <CollapsibleContent>
        <div className="pb-2">{children}</div>
      </CollapsibleContent>
    </Collapsible>
  )
}

const NONE = <span className="text-muted-foreground">—</span>

/** A setting the overview loads on its own, so it can be pending or fail. */
export type LoadedSetting<T> = {
  data: T | undefined
  isLoading: boolean
  isError: boolean
  refetch: () => void
}

export function SettingUnavailable({
  onRetry,
  what,
}: {
  onRetry: () => void
  what: string
}) {
  return (
    <span className="text-destructive">
      Could not load {what}.{' '}
      <Button
        variant="link"
        size="xs"
        className="h-auto px-0 align-baseline text-sm text-destructive underline"
        onClick={() => onRetry()}
      >
        Retry
      </Button>
    </span>
  )
}

/** Skeleton while loading and Retry on failure, never a false "Not set up". */
function SettingBody<T>({
  children,
  setting,
  what,
}: {
  children: (data: T) => ReactNode
  setting: LoadedSetting<T>
  what: string
}) {
  if (setting.data !== undefined) return children(setting.data)
  if (setting.isError) {
    return (
      <p className="py-1 text-sm">
        <SettingUnavailable what={what} onRetry={setting.refetch} />
      </p>
    )
  }
  if (setting.isLoading) return <Skeleton className="my-1 h-5 w-48" />
  return (
    <Text variant="muted" className="py-1">
      Not set up
    </Text>
  )
}

function KV({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex gap-4 py-1 text-sm max-sm:flex-col max-sm:gap-0.5">
      <span className="w-40 shrink-0 text-muted-foreground">{label}</span>
      <span className="min-w-0 break-all">{children}</span>
    </div>
  )
}

export function PipelineConfigurationCard({
  androidSigning,
  iosSigning,
  manualOnlyTriggers,
  pipeline,
  publishing = [],
}: {
  /** Null when the viewer cannot read signing, so the section is hidden. */
  androidSigning: LoadedSetting<PipelineAndroidSigningResponse> | null
  iosSigning: LoadedSetting<PipelineIosSigningResponse> | null
  manualOnlyTriggers: boolean
  pipeline: Pipeline
  /** One line per store the pipeline can publish to. */
  publishing?: Array<{ label: string; value: ReactNode }>
}) {
  return (
    <Card>
      <CardContent className="divide-y">
        {publishing.length > 0 ? (
          <Section title="Publishing" defaultOpen>
            {publishing.map((line) => (
              <KV key={line.label} label={line.label}>
                {line.value}
              </KV>
            ))}
          </Section>
        ) : null}
        <Section title="Configuration" defaultOpen>
          <KV label="Config path">
            <span className="font-mono">{pipeline.config_path}</span>
          </KV>
          <KV label="Resolution">
            {pipeline.config_path_explicit
              ? 'Explicit path only'
              : 'Auto-detect .oore.yaml / .oore.yml'}
          </KV>
          <KV label="Flutter version">
            <span className="font-mono">
              {pipeline.execution_config.flutter_version || (
                <span className="font-sans">From .fvmrc, or stable</span>
              )}
            </span>
          </KV>
          <KV label="Created">
            <RelativeTime at={pipeline.created_at} />
          </KV>
          <KV label="Updated">
            <RelativeTime at={pipeline.updated_at} />
          </KV>
        </Section>

        <Section title="Triggers">
          {manualOnlyTriggers ? (
            <KV label="Mode">Manual only</KV>
          ) : (
            <>
              <KV label="Events">
                {pipeline.trigger_config.events.length > 0 ? (
                  <div className="flex flex-wrap gap-1">
                    {pipeline.trigger_config.events.map((event) => (
                      <Badge
                        key={event}
                        variant="outline"
                        className="text-[11px]"
                      >
                        {formatEnumLabel(event)}
                      </Badge>
                    ))}
                  </div>
                ) : (
                  'Every repository event'
                )}
              </KV>
              <KV label="Branch patterns">
                {pipeline.trigger_config.branches.length > 0 ? (
                  <div className="flex flex-wrap gap-1">
                    {pipeline.trigger_config.branches.map((branch) => (
                      <Badge
                        key={branch}
                        variant="outline"
                        className="font-mono text-[11px]"
                      >
                        {branch}
                      </Badge>
                    ))}
                  </div>
                ) : (
                  'All branches'
                )}
              </KV>
            </>
          )}
          <KV label="Cancel previous">
            {pipeline.concurrency.cancel_previous ? 'Yes' : 'No'}
          </KV>
          <KV label="Max concurrent">
            {pipeline.concurrency.max_concurrent ?? 'No limit'}
          </KV>
        </Section>

        <Section title="Execution config">
          <KV label="Pre-build">
            <span className="font-mono">
              {pipeline.execution_config.commands.pre_build.length > 0
                ? pipeline.execution_config.commands.pre_build.join(' && ')
                : NONE}
            </span>
          </KV>
          <KV label="Build">
            <span className="font-mono">
              {pipeline.execution_config.commands.build.length > 0
                ? pipeline.execution_config.commands.build.join(' && ')
                : NONE}
            </span>
          </KV>
          <KV label="Post-build">
            <span className="font-mono">
              {pipeline.execution_config.commands.post_build.length > 0
                ? pipeline.execution_config.commands.post_build.join(' && ')
                : NONE}
            </span>
          </KV>
          {(pipeline.execution_config.platform_build_args?.android.length ??
            0) > 0 ? (
            <KV label="Android args">
              <span className="font-mono">
                {pipeline.execution_config.platform_build_args?.android.join(
                  ' ',
                )}
              </span>
            </KV>
          ) : null}
          {(pipeline.execution_config.platform_build_args?.ios.length ?? 0) >
          0 ? (
            <KV label="iOS args">
              <span className="font-mono">
                {pipeline.execution_config.platform_build_args?.ios.join(' ')}
              </span>
            </KV>
          ) : null}
          {(pipeline.execution_config.platform_build_args?.macos.length ?? 0) >
          0 ? (
            <KV label="macOS args">
              <span className="font-mono">
                {pipeline.execution_config.platform_build_args?.macos.join(' ')}
              </span>
            </KV>
          ) : null}
          {pipeline.execution_config.platform_commands?.android ||
          pipeline.execution_config.platform_commands?.ios ||
          pipeline.execution_config.platform_commands?.macos ? (
            <KV label="Command overrides">
              <span className="font-mono">
                {[
                  pipeline.execution_config.platform_commands.android
                    ? `Android: ${pipeline.execution_config.platform_commands.android}`
                    : '',
                  pipeline.execution_config.platform_commands.ios
                    ? `iOS: ${pipeline.execution_config.platform_commands.ios}`
                    : '',
                  pipeline.execution_config.platform_commands.macos
                    ? `macOS: ${pipeline.execution_config.platform_commands.macos}`
                    : '',
                ]
                  .filter(Boolean)
                  .join(', ')}
              </span>
            </KV>
          ) : null}
          <KV label="Env vars">
            {(pipeline.execution_config.env?.length ?? 0) > 0
              ? `${pipeline.execution_config.env!.length} configured`
              : NONE}
          </KV>
          <KV label="Artifact patterns">
            <span className="font-mono">
              {pipeline.execution_config.artifact_patterns.length > 0
                ? pipeline.execution_config.artifact_patterns.join(', ')
                : NONE}
            </span>
          </KV>
        </Section>

        {androidSigning &&
        pipeline.execution_config.platforms.includes('android') ? (
          <Section title="Android signing">
            <SettingBody setting={androidSigning} what="Android signing">
              {(androidSigning) => (
                <>
                  <KV label="Release">
                    {androidSigning.release.enabled
                      ? `On · ${androidSigning.release.keystore_filename ?? 'keystore stored'}`
                      : 'Off'}
                  </KV>
                  <KV label="Debug">
                    {androidSigning.debug.enabled
                      ? `On · ${androidSigning.debug.keystore_filename ?? 'keystore stored'}`
                      : 'Off'}
                  </KV>
                </>
              )}
            </SettingBody>
          </Section>
        ) : null}

        {iosSigning && pipeline.execution_config.platforms.includes('ios') ? (
          <Section title="iOS signing">
            <SettingBody setting={iosSigning} what="iOS signing">
              {(iosSigning) => (
                <>
                  <KV label="Status">{iosSigning.enabled ? 'On' : 'Off'}</KV>
                  {iosSigning.enabled ? (
                    <>
                      <KV label="Mode">
                        {iosSigning.mode === 'manual'
                          ? 'Manual (.p12 + provisioning profiles)'
                          : iosSigning.mode === 'api'
                            ? 'API (App Store Connect)'
                            : 'Hybrid (manual cert + API automation)'}
                      </KV>
                      {iosSigning.team_id ? (
                        <KV label="Team ID">
                          <span className="font-mono">
                            {iosSigning.team_id}
                          </span>
                        </KV>
                      ) : null}
                      {iosSigning.bundle_ids.length > 0 ? (
                        <KV label="Bundle IDs">
                          <div className="flex flex-wrap gap-1">
                            {iosSigning.bundle_ids.map((id) => (
                              <Badge
                                key={id}
                                variant="outline"
                                className="font-mono text-[11px]"
                              >
                                {id}
                              </Badge>
                            ))}
                          </div>
                        </KV>
                      ) : null}
                      {iosSigning.mode === 'manual' ||
                      iosSigning.mode === 'hybrid' ? (
                        <KV label="Certificate">
                          {iosSigning.has_p12
                            ? (iosSigning.p12_filename ?? 'Stored')
                            : 'Not uploaded'}
                        </KV>
                      ) : null}
                      {iosSigning.mode === 'api' ||
                      iosSigning.mode === 'hybrid' ? (
                        <KV label="API key">
                          {iosSigning.has_api_key
                            ? `Key ${iosSigning.api_key_id ?? 'stored'}`
                            : 'Not set up'}
                        </KV>
                      ) : null}
                      {iosSigning.provisioning_profiles.length > 0 ? (
                        <KV label="Profiles">
                          {iosSigning.provisioning_profiles.length} provisioning
                          profile
                          {iosSigning.provisioning_profiles.length === 1
                            ? ''
                            : 's'}
                        </KV>
                      ) : null}
                    </>
                  ) : null}
                </>
              )}
            </SettingBody>
          </Section>
        ) : null}
      </CardContent>
    </Card>
  )
}
