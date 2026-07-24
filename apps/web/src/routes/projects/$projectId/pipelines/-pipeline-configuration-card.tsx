import { useState } from 'react'
import type { ReactNode } from 'react'
import {
  ArrowDown as ArrowDown01Icon,
  ArrowRight as ArrowRight01Icon,
} from 'lucide-react'
import { Badge } from '@/components/ui/badge'
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
} from '@/lib/types'

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
        <SectionIcon size={14} />
        {title}
      </CollapsibleTrigger>
      <CollapsibleContent>
        <div className="pb-2">{children}</div>
      </CollapsibleContent>
    </Collapsible>
  )
}

function KV({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex gap-4 py-1 text-xs">
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
}: {
  androidSigning: PipelineAndroidSigningResponse | undefined
  iosSigning: PipelineIosSigningResponse | undefined
  manualOnlyTriggers: boolean
  pipeline: Pipeline
}) {
  return (
    <Card>
      <CardContent className="divide-y">
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
              {pipeline.execution_config.flutter_version || 'auto'}
            </span>
          </KV>
          <KV label="Created">
            {new Date(pipeline.created_at * 1000).toLocaleString()}
          </KV>
          <KV label="Updated">
            {new Date(pipeline.updated_at * 1000).toLocaleString()}
          </KV>
        </Section>

        <Section title="Triggers">
          {manualOnlyTriggers ? (
            <KV label="Mode">manual only</KV>
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
                        {event}
                      </Badge>
                    ))}
                  </div>
                ) : (
                  'all events'
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
                  'all branches'
                )}
              </KV>
            </>
          )}
          <KV label="Cancel previous">
            {pipeline.concurrency.cancel_previous ? 'yes' : 'no'}
          </KV>
          <KV label="Max concurrent">
            {pipeline.concurrency.max_concurrent ?? 'unlimited'}
          </KV>
        </Section>

        <Section title="Execution config">
          <KV label="Pre-build">
            <span className="font-mono">
              {pipeline.execution_config.commands.pre_build.length > 0
                ? pipeline.execution_config.commands.pre_build.join(' && ')
                : 'none'}
            </span>
          </KV>
          <KV label="Build">
            <span className="font-mono">
              {pipeline.execution_config.commands.build.length > 0
                ? pipeline.execution_config.commands.build.join(' && ')
                : 'none'}
            </span>
          </KV>
          <KV label="Post-build">
            <span className="font-mono">
              {pipeline.execution_config.commands.post_build.length > 0
                ? pipeline.execution_config.commands.post_build.join(' && ')
                : 'none'}
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
                    ? `android: ${pipeline.execution_config.platform_commands.android}`
                    : '',
                  pipeline.execution_config.platform_commands.ios
                    ? `ios: ${pipeline.execution_config.platform_commands.ios}`
                    : '',
                  pipeline.execution_config.platform_commands.macos
                    ? `macos: ${pipeline.execution_config.platform_commands.macos}`
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
              : 'none'}
          </KV>
          <KV label="Artifact patterns">
            <span className="font-mono">
              {pipeline.execution_config.artifact_patterns.length > 0
                ? pipeline.execution_config.artifact_patterns.join(', ')
                : 'none'}
            </span>
          </KV>
        </Section>

        {pipeline.execution_config.platforms.includes('android') ? (
          <Section title="Android signing">
            {androidSigning ? (
              <>
                <KV label="Release">
                  {androidSigning.release.enabled
                    ? `enabled (${androidSigning.release.keystore_filename ?? 'keystore configured'})`
                    : 'disabled'}
                </KV>
                <KV label="Debug">
                  {androidSigning.debug.enabled
                    ? `enabled (${androidSigning.debug.keystore_filename ?? 'keystore configured'})`
                    : 'disabled'}
                </KV>
              </>
            ) : (
              <p className="py-1 text-xs text-muted-foreground">
                Not configured
              </p>
            )}
          </Section>
        ) : null}

        {pipeline.execution_config.platforms.includes('ios') ? (
          <Section title="iOS signing">
            {iosSigning ? (
              <>
                <KV label="Status">
                  {iosSigning.enabled ? 'enabled' : 'disabled'}
                </KV>
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
                        <span className="font-mono">{iosSigning.team_id}</span>
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
                          ? (iosSigning.p12_filename ?? 'configured')
                          : 'not uploaded'}
                      </KV>
                    ) : null}
                    {iosSigning.mode === 'api' ||
                    iosSigning.mode === 'hybrid' ? (
                      <KV label="API key">
                        {iosSigning.has_api_key
                          ? `Key ${iosSigning.api_key_id ?? 'configured'}`
                          : 'not configured'}
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
            ) : (
              <p className="py-1 text-xs text-muted-foreground">
                Not configured
              </p>
            )}
          </Section>
        ) : null}
      </CardContent>
    </Card>
  )
}
