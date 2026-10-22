import { useState } from 'react'
import { useFormContext } from 'react-hook-form'
import { HugeiconsIcon } from '@hugeicons/react'
import { ArrowDown01Icon, ArrowRight01Icon } from '@hugeicons/core-free-icons'

import type { PipelineFormValues } from '@/lib/pipeline-schema'
import { buildsAppBundle } from '@/lib/pipeline-form-utils'
import { useRunners } from '@/hooks/use-runners'
import {
  PipelineFormSection,
  PipelineFormSubsection,
} from '@/components/pipeline-form-section'
import { SettingsSurfaceSeparator } from '@/components/settings/settings-section'
import SetupHint from '@/components/setup-hint'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible'
import {
  FormControl,
  FormDescription,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
} from '@/components/ui/form'
import { Input } from '@/components/ui/input'
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Textarea } from '@/components/ui/textarea'

const ISOLATION_OPTIONS = {
  host: 'On the runner, with its installed toolchains',
  vm: "In a fresh macOS VM for the pipeline's Flutter version",
} satisfies Record<PipelineFormValues['isolation'], string>

const ISOLATION_EFFECTS = {
  host: 'Builds share the runner’s Flutter, Xcode and caches, so they start fast. Keep the runner’s toolchains up to date.',
  vm: 'A VM starts clean every build and needs a runner with Tart and the image for that Flutter version.',
} satisfies Record<PipelineFormValues['isolation'], string>

type PlatformFlag = 'platform_android' | 'platform_ios' | 'platform_macos'
type PlatformFieldName =
  | 'android_build_args'
  | 'ios_build_args'
  | 'macos_build_args'
  | 'android_command_override'
  | 'ios_command_override'
  | 'macos_command_override'

const ARGS_PLACEHOLDER = 'One argument per line, e.g. --flavor=prod'
const OVERRIDE_PLACEHOLDER = 'Empty uses the default command'

const PLATFORM_ARGUMENT_FIELDS = [
  ['android_build_args', 'Android build args', 'platform_android'],
  ['ios_build_args', 'iOS build args', 'platform_ios'],
  ['macos_build_args', 'macOS build args', 'platform_macos'],
] as const satisfies ReadonlyArray<
  readonly [PlatformFieldName, string, PlatformFlag]
>

const PLATFORM_COMMAND_OVERRIDES = [
  ['android_command_override', 'Android command override', 'platform_android'],
  ['ios_command_override', 'iOS command override', 'platform_ios'],
  ['macos_command_override', 'macOS command override', 'platform_macos'],
] as const satisfies ReadonlyArray<
  readonly [PlatformFieldName, string, PlatformFlag]
>

/**
 * Build: everything that runs on the runner, in one section. Commands,
 * environment and artifacts are visible; per-platform arguments and command
 * overrides sit behind one disclosure because most pipelines never touch
 * them.
 */
export function PipelineBuildSection({
  advancedOpen,
  commandCount,
  envVarCount,
  playBundle,
  previewDefaults,
}: {
  advancedOpen: boolean
  commandCount: number
  envVarCount: number
  /** Google Play upload is on, so the runner adds an App Bundle build. */
  playBundle: boolean
  previewDefaults: Array<string>
}) {
  const form = useFormContext<PipelineFormValues>()
  const customizationEnabled = form.watch('enable_customization')
  const values = form.watch()
  const [platformOpen, setPlatformOpen] = useState(advancedOpen)
  const PlatformIcon = platformOpen ? ArrowDown01Icon : ArrowRight01Icon
  // Show a platform's fields when it is selected, or when it still holds a
  // value that would otherwise be invisible.
  const showPlatformField = (name: PlatformFieldName, platform: PlatformFlag) =>
    values[platform] || (values[name] ?? '').trim().length > 0
  const argumentFields = PLATFORM_ARGUMENT_FIELDS.filter(([name, , platform]) =>
    showPlatformField(name, platform),
  )
  const overrideFields = PLATFORM_COMMAND_OVERRIDES.filter(
    ([name, , platform]) => showPlatformField(name, platform),
  )
  const androidOverride = (values.android_command_override ?? '').trim()

  return (
    <PipelineFormSection
      id="build"
      title="Build"
      description="What runs on the runner. Defaults work for a standard Flutter app."
    >
      <PipelineFormSubsection
        title="Commands"
        description={
          commandCount > 0
            ? `${commandCount} custom command${commandCount !== 1 ? 's' : ''} around the default platform build.`
            : 'Platform defaults, with optional commands before and after.'
        }
      >
        {previewDefaults.length > 0 ? (
          <div
            role="note"
            aria-label="Platform build commands"
            className="flex flex-col gap-1.5 rounded-2xl bg-muted/50 px-4 py-3 text-sm"
          >
            <p className="font-medium">Platform build commands</p>
            <ul className="flex flex-col gap-0.5">
              {previewDefaults.map((command) => (
                <li
                  key={command}
                  className="font-mono text-[13px] wrap-break-word"
                >
                  {command}
                </li>
              ))}
            </ul>
            {playBundle && !androidOverride ? (
              <p className="text-muted-foreground">
                Google Play upload is on, so Android builds an App Bundle as
                well as the APK.
              </p>
            ) : null}
            {playBundle &&
            androidOverride &&
            !buildsAppBundle(androidOverride) ? (
              <p className="text-warning">
                Google Play upload is on, but the Android command override does
                not build an App Bundle, so the upload will fail. Use{' '}
                <span className="font-mono">flutter build appbundle</span> or
                clear the override.
              </p>
            ) : null}
          </div>
        ) : null}
        <FormField
          control={form.control}
          name="enable_customization"
          render={({ field }) => (
            <FormItem>
              <label className="flex items-center gap-2 text-sm">
                <Checkbox
                  checked={field.value}
                  onCheckedChange={(checked) => field.onChange(!!checked)}
                />
                Add custom build commands
              </label>
            </FormItem>
          )}
        />

        {customizationEnabled ? (
          <>
            <CommandField
              name="pre_build_commands"
              label="Pre-build commands"
              placeholder={'One command per line\nexample: dart --version'}
            />
            <CommandField
              name="build_commands"
              label="Extra build commands"
              placeholder={'One command per line\nexample: flutter test'}
              description="Runs after the default platform commands."
            />
            <CommandField
              name="post_build_commands"
              label="Post-build commands"
              placeholder={'One command per line\nexample: ls -la build/'}
            />
          </>
        ) : null}

        <Collapsible open={platformOpen} onOpenChange={setPlatformOpen}>
          <CollapsibleTrigger
            render={
              <Button
                type="button"
                variant="ghost"
                size="sm"
                className="-ml-2"
              />
            }
          >
            <HugeiconsIcon icon={PlatformIcon} />
            Per-platform arguments and overrides
          </CollapsibleTrigger>
          <CollapsibleContent>
            <div className="flex flex-col gap-4 pt-3">
              {argumentFields.length > 0 ? (
                <p className="text-sm text-muted-foreground">
                  Each line is appended to the platform&apos;s default Flutter
                  build command.
                </p>
              ) : null}
              {argumentFields.map(([name, label]) => (
                <FormField
                  key={name}
                  control={form.control}
                  name={name}
                  render={({ field }) => (
                    <FormItem>
                      <FormLabel>{label}</FormLabel>
                      <FormControl>
                        <Textarea
                          placeholder={ARGS_PLACEHOLDER}
                          className="font-mono"
                          {...field}
                        />
                      </FormControl>
                      <FormMessage />
                    </FormItem>
                  )}
                />
              ))}
              {overrideFields.length > 0 ? (
                <p className="text-sm text-muted-foreground">
                  A command override replaces the default platform command
                  entirely.
                </p>
              ) : null}
              {overrideFields.map(([name, label]) => (
                <FormField
                  key={name}
                  control={form.control}
                  name={name}
                  render={({ field }) => (
                    <FormItem>
                      <FormLabel>{label}</FormLabel>
                      <FormControl>
                        <Input
                          placeholder={OVERRIDE_PLACEHOLDER}
                          className="font-mono"
                          {...field}
                        />
                      </FormControl>
                      <FormMessage />
                    </FormItem>
                  )}
                />
              ))}
              {argumentFields.length === 0 && overrideFields.length === 0 ? (
                <p className="text-sm text-muted-foreground">
                  Choose a platform under Basics to add arguments or overrides.
                </p>
              ) : null}
            </div>
          </CollapsibleContent>
        </Collapsible>
      </PipelineFormSubsection>

      <SettingsSurfaceSeparator />

      <PipelineFormSubsection
        title="Environment variables"
        description={
          envVarCount > 0
            ? `${envVarCount} variable${envVarCount !== 1 ? 's' : ''}, available in every build step.`
            : 'App-specific values, available in every build step.'
        }
      >
        <FormField
          control={form.control}
          name="env_vars"
          render={({ field }) => (
            <FormItem>
              <FormLabel className="sr-only">Environment variables</FormLabel>
              <FormControl>
                <Textarea
                  placeholder={
                    'API_BASE_URL=https://api.example.com\nAPP_FLAVOR=dev'
                  }
                  className="font-mono"
                  rows={4}
                  {...field}
                />
              </FormControl>
              <FormDescription>One KEY=VALUE per line.</FormDescription>
              <FormMessage />
            </FormItem>
          )}
        />
        <SetupHint
          title="Built-in CI variables"
          items={[
            <span key="built-in-variables">
              Oore also injects <code>PROJECT_ID</code>,{' '}
              <code>PIPELINE_ID</code>, <code>BUILD_ID</code>,{' '}
              <code>PROJECT_BUILD_NUMBER</code>, <code>BUILD_NUMBER</code>,{' '}
              <code>OORE_VERSION_CODE</code> (build number plus offset),{' '}
              <code>OORE_VERSION_NAME</code> (from <code>pubspec.yaml</code>),{' '}
              <code>CI=true</code>, and branch/commit values when present.
            </span>,
            'Use this section for app-specific values such as API_BASE_URL, APP_FLAVOR, SENTRY_DSN, or feature flags.',
          ]}
        />
      </PipelineFormSubsection>

      <SettingsSurfaceSeparator />

      <PipelineFormSubsection
        title="Artifacts"
        description="Files to keep from the build output. These become the installable apps."
      >
        <FormField
          control={form.control}
          name="artifact_patterns"
          render={({ field }) => (
            <FormItem>
              <FormLabel className="sr-only">Artifact patterns</FormLabel>
              <FormControl>
                <Textarea
                  placeholder={'One pattern per line\n*.apk\n*.ipa'}
                  className="font-mono"
                  rows={3}
                  {...field}
                />
              </FormControl>
              <FormDescription>
                One glob per line, relative to the repository root. Empty uses
                the platform defaults.
              </FormDescription>
              <FormMessage />
            </FormItem>
          )}
        />
      </PipelineFormSubsection>

      <SettingsSurfaceSeparator />

      <RunnerSubsection />
    </PipelineFormSection>
  )
}

/** Where the build runs: which runner takes it and whether it gets a VM. */
function RunnerSubsection() {
  const form = useFormContext<PipelineFormValues>()
  const { data: knownLabels } = useRunners(
    { limit: 100 },
    {
      select: (data) =>
        [...new Set(data.runners.flatMap((runner) => runner.labels ?? []))]
          .sort()
          .join(', '),
    },
  )
  return (
    <PipelineFormSubsection
      title="Runner"
      description="Which runner may take a build and how isolated it is."
    >
      <FormField
        control={form.control}
        name="isolation"
        render={({ field }) => (
          <FormItem>
            <FormLabel>Isolation</FormLabel>
            <Select
              value={field.value}
              onValueChange={field.onChange}
              items={ISOLATION_OPTIONS}
            >
              <FormControl>
                <SelectTrigger className="w-full" aria-label="Isolation">
                  <SelectValue />
                </SelectTrigger>
              </FormControl>
              <SelectContent>
                <SelectGroup>
                  {Object.entries(ISOLATION_OPTIONS).map(([key, label]) => (
                    <SelectItem key={key} value={key}>
                      {label}
                    </SelectItem>
                  ))}
                </SelectGroup>
              </SelectContent>
            </Select>
            <FormDescription>{ISOLATION_EFFECTS[field.value]}</FormDescription>
            <FormMessage />
          </FormItem>
        )}
      />
      <FormField
        control={form.control}
        name="runs_on"
        render={({ field }) => (
          <FormItem>
            <FormLabel>Runner labels (optional)</FormLabel>
            <FormControl>
              <Input
                placeholder="macos, xcode-16"
                className="font-mono"
                {...field}
              />
            </FormControl>
            <FormDescription>
              Comma-separated. A build waits for a runner that has every label.
              Empty means <span className="font-mono">macos</span>.
              {knownLabels ? (
                <>
                  {' '}
                  Connected runners report:{' '}
                  <span className="font-mono">{knownLabels}</span>.
                </>
              ) : null}
            </FormDescription>
            <FormMessage />
          </FormItem>
        )}
      />
    </PipelineFormSubsection>
  )
}

function CommandField({
  description,
  label,
  name,
  placeholder,
}: {
  description?: string
  label: string
  name: 'pre_build_commands' | 'build_commands' | 'post_build_commands'
  placeholder: string
}) {
  const form = useFormContext<PipelineFormValues>()
  return (
    <FormField
      control={form.control}
      name={name}
      render={({ field }) => (
        <FormItem>
          <FormLabel>{label}</FormLabel>
          <FormControl>
            <Textarea
              placeholder={placeholder}
              className="font-mono"
              {...field}
            />
          </FormControl>
          {description ? (
            <FormDescription>{description}</FormDescription>
          ) : null}
          <FormMessage />
        </FormItem>
      )}
    />
  )
}
