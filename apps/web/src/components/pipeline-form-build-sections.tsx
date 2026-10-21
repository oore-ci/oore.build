import { useState } from 'react'
import { useFormContext } from 'react-hook-form'
import { HugeiconsIcon } from '@hugeicons/react'
import { ArrowDown01Icon, ArrowRight01Icon } from '@hugeicons/core-free-icons'

import type { PipelineFormValues } from '@/lib/pipeline-schema'
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

const PLATFORM_ARGUMENT_FIELDS = [
  [
    'android_build_args',
    'Android build args',
    '--dart-define-from-file=config/dev.json\n--obfuscate',
  ],
  [
    'ios_build_args',
    'iOS build args',
    '--flavor=production\n--build-number=$OORE_VERSION_CODE',
  ],
  ['macos_build_args', 'macOS build args', '--build-number=$OORE_VERSION_CODE'],
] as const

const PLATFORM_COMMAND_OVERRIDES = [
  [
    'android_command_override',
    'Android command override',
    'flutter build appbundle --release',
  ],
  [
    'ios_command_override',
    'iOS command override',
    'flutter build ios --release --no-codesign',
  ],
  [
    'macos_command_override',
    'macOS command override',
    'flutter build macos --release',
  ],
] as const

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
}: {
  advancedOpen: boolean
  commandCount: number
  envVarCount: number
}) {
  const form = useFormContext<PipelineFormValues>()
  const customizationEnabled = form.watch('enable_customization')
  const [platformOpen, setPlatformOpen] = useState(advancedOpen)
  const PlatformIcon = platformOpen ? ArrowDown01Icon : ArrowRight01Icon

  return (
    <PipelineFormSection
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
              <p className="text-sm text-muted-foreground">
                Each line is appended to the platform&apos;s default Flutter
                build command.
              </p>
              {PLATFORM_ARGUMENT_FIELDS.map(([name, label, placeholder]) => (
                <FormField
                  key={name}
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
                      <FormMessage />
                    </FormItem>
                  )}
                />
              ))}
              <p className="text-sm text-muted-foreground">
                A command override replaces the default platform command
                entirely.
              </p>
              {PLATFORM_COMMAND_OVERRIDES.map(([name, label, placeholder]) => (
                <FormField
                  key={name}
                  control={form.control}
                  name={name}
                  render={({ field }) => (
                    <FormItem>
                      <FormLabel>{label}</FormLabel>
                      <FormControl>
                        <Input
                          placeholder={placeholder}
                          className="font-mono"
                          {...field}
                        />
                      </FormControl>
                      <FormMessage />
                    </FormItem>
                  )}
                />
              ))}
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
            <FormDescription>
              A VM starts clean every build and needs a runner with Tart and the
              image for that Flutter version.
            </FormDescription>
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
