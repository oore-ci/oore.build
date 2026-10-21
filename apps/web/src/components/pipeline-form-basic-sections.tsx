import { useFormContext } from 'react-hook-form'
import { HugeiconsIcon } from '@hugeicons/react'
import { AlertCircleIcon } from '@hugeicons/core-free-icons'

import type { PipelineFormValues } from '@/lib/pipeline-schema'
import { TRIGGER_EVENTS } from '@/lib/pipeline-schema'
import { formatEnumLabel } from '@/lib/format-utils'
import {
  PipelineFormSection,
  PipelineFormSubsection,
} from '@/components/pipeline-form-section'
import { SettingsSurfaceSeparator } from '@/components/settings/settings-section'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Checkbox } from '@/components/ui/checkbox'
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

const CONFIG_SOURCES = {
  auto: 'Use repo config if found (.oore.yaml, .oore.yml)',
  explicit: 'Choose a config file',
} satisfies Record<string, string>

const PLATFORM_FIELDS = [
  ['platform_android', 'Android'],
  ['platform_ios', 'iOS'],
  ['platform_macos', 'macOS'],
] as const

/**
 * Basics: what the pipeline is called, what it builds, and where its config
 * comes from. When a repository workflow owns execution, the workflow summary
 * replaces the config fields.
 */
export function PipelineBasicsSection({
  configMode,
  previewDefaults,
  repositoryWorkflow,
}: {
  configMode: PipelineFormValues['config_mode']
  previewDefaults: Array<string>
  repositoryWorkflow?: React.ReactNode
}) {
  const form = useFormContext<PipelineFormValues>()
  return (
    <PipelineFormSection
      title="Basics"
      description="Name the pipeline and choose what it builds."
    >
      <FormField
        control={form.control}
        name="name"
        render={({ field }) => (
          <FormItem>
            <FormLabel>Pipeline name</FormLabel>
            <FormControl>
              <Input placeholder="Release" autoFocus {...field} />
            </FormControl>
            <FormMessage />
          </FormItem>
        )}
      />

      <SettingsSurfaceSeparator />

      {repositoryWorkflow ? (
        <PipelineFormSubsection
          title="Configuration"
          description="Build settings come from the workflow checked into the repository."
        >
          {repositoryWorkflow}
        </PipelineFormSubsection>
      ) : (
        <>
          <PipelineFormSubsection
            title="Platforms"
            description="Which platforms this pipeline builds."
          >
            <div className="grid grid-cols-1 gap-2 sm:grid-cols-3">
              {PLATFORM_FIELDS.map(([name, label]) => (
                <FormField
                  key={name}
                  control={form.control}
                  name={name}
                  render={({ field }) => (
                    <label className="flex items-center gap-2 text-sm">
                      <Checkbox
                        checked={field.value}
                        onCheckedChange={(checked) => field.onChange(!!checked)}
                      />
                      {label}
                    </label>
                  )}
                />
              ))}
            </div>
          </PipelineFormSubsection>

          <SettingsSurfaceSeparator />

          <PipelineFormSubsection
            title="Configuration"
            description="Where the build configuration comes from and which Flutter to use."
          >
            <FormField
              control={form.control}
              name="config_mode"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>Config source</FormLabel>
                  <Select
                    value={field.value}
                    onValueChange={field.onChange}
                    items={CONFIG_SOURCES}
                  >
                    <FormControl>
                      <SelectTrigger
                        className="w-full"
                        aria-label="Config source"
                      >
                        <SelectValue placeholder="Choose source" />
                      </SelectTrigger>
                    </FormControl>
                    <SelectContent>
                      <SelectGroup>
                        {Object.entries(CONFIG_SOURCES).map(([key, value]) => (
                          <SelectItem key={key} value={key}>
                            {value}
                          </SelectItem>
                        ))}
                      </SelectGroup>
                    </SelectContent>
                  </Select>
                  <FormMessage />
                </FormItem>
              )}
            />

            {configMode === 'explicit' ? (
              <FormField
                control={form.control}
                name="config_path"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel>Config path</FormLabel>
                    <FormControl>
                      <Input
                        placeholder="ci/oore.yaml"
                        className="font-mono"
                        {...field}
                      />
                    </FormControl>
                    <FormMessage />
                  </FormItem>
                )}
              />
            ) : null}

            <FormField
              control={form.control}
              name="flutter_version"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>Flutter version (optional)</FormLabel>
                  <FormControl>
                    <Input
                      placeholder="3.24.0 (or stable)"
                      className="font-mono"
                      {...field}
                    />
                  </FormControl>
                  <FormDescription>
                    Oore uses <span className="font-mono">.fvmrc</span> when
                    present, then this value, then stable. VM builds use the
                    image for that version; host builds use the runner's
                    installed Flutter.
                  </FormDescription>
                  <FormMessage />
                </FormItem>
              )}
            />

            <FormField
              control={form.control}
              name="build_number_offset"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>Build number offset (optional)</FormLabel>
                  <FormControl>
                    <Input
                      inputMode="numeric"
                      placeholder="0"
                      className="font-mono"
                      {...field}
                    />
                  </FormControl>
                  <FormDescription>
                    Added to the build number to form{' '}
                    <span className="font-mono">OORE_VERSION_CODE</span>. Set it
                    above the version code your app already has in the store,
                    then pass{' '}
                    <span className="font-mono">
                      --build-number=$OORE_VERSION_CODE
                    </span>{' '}
                    to <span className="font-mono">flutter build</span>.
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
                      placeholder="macos"
                      className="font-mono"
                      {...field}
                    />
                  </FormControl>
                  <FormDescription>
                    Builds run only on runners that have every label listed,
                    such as <span className="font-mono">macos, xcode-16</span>.
                    Leave empty to use any macOS runner.
                  </FormDescription>
                  <FormMessage />
                </FormItem>
              )}
            />

            {previewDefaults.length > 0 ? (
              <Alert>
                <AlertTitle>Default build commands</AlertTitle>
                <AlertDescription>
                  <ul className="list-disc space-y-0.5 pl-4">
                    {previewDefaults.map((command) => (
                      <li key={command} className="font-mono">
                        {command}
                      </li>
                    ))}
                  </ul>
                </AlertDescription>
              </Alert>
            ) : null}
          </PipelineFormSubsection>
        </>
      )}
    </PipelineFormSection>
  )
}

export function PipelineTriggersSection({
  manualOnlyTriggers,
}: {
  manualOnlyTriggers: boolean
}) {
  const form = useFormContext<PipelineFormValues>()
  const selectedEvents = form.watch('trigger_events')
  const cancelPrevious = form.watch('cancel_previous')

  function toggleEvent(event: (typeof TRIGGER_EVENTS)[number]) {
    form.setValue(
      'trigger_events',
      selectedEvents.includes(event)
        ? selectedEvents.filter((entry) => entry !== event)
        : [...selectedEvents, event],
      { shouldDirty: true },
    )
  }

  return (
    <PipelineFormSection
      title="Triggers"
      description="When this pipeline runs on its own. You can always run it by hand."
    >
      {manualOnlyTriggers ? (
        <Alert>
          <HugeiconsIcon icon={AlertCircleIcon} size={16} />
          <AlertDescription>
            This repository uses local Git, so builds can only be started from
            the UI or API. Webhook triggers need a connected GitHub or GitLab
            source.
          </AlertDescription>
        </Alert>
      ) : (
        <>
          <PipelineFormSubsection
            title="Events"
            description="Repository events that start a build."
          >
            <div className="grid grid-cols-1 gap-2 sm:grid-cols-3">
              {TRIGGER_EVENTS.map((event) => (
                <label key={event} className="flex items-center gap-2 text-sm">
                  <Checkbox
                    checked={selectedEvents.includes(event)}
                    onCheckedChange={() => toggleEvent(event)}
                  />
                  {formatEnumLabel(event)}
                </label>
              ))}
            </div>
            <FormField
              control={form.control}
              name="branches"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>Branch patterns (optional)</FormLabel>
                  <FormControl>
                    <Input
                      placeholder="main, develop, release/*"
                      className="font-mono"
                      {...field}
                    />
                  </FormControl>
                  <FormDescription>
                    Comma-separated. Leave empty to build every branch.
                  </FormDescription>
                  <FormMessage />
                </FormItem>
              )}
            />
          </PipelineFormSubsection>

          <SettingsSurfaceSeparator />
        </>
      )}

      <PipelineFormSubsection
        title="Concurrency"
        description="How overlapping builds of this pipeline are handled."
      >
        <label className="flex items-center gap-2 text-sm">
          <Checkbox
            checked={cancelPrevious}
            onCheckedChange={(checked) =>
              form.setValue('cancel_previous', !!checked, {
                shouldDirty: true,
              })
            }
          />
          Cancel previous builds on the same branch
        </label>

        <FormField
          control={form.control}
          name="max_concurrent"
          render={({ field }) => (
            <FormItem>
              <FormLabel>Max concurrent builds (optional)</FormLabel>
              <FormControl>
                <Input
                  type="number"
                  min={1}
                  max={100}
                  className="sm:max-w-40"
                  {...field}
                />
              </FormControl>
              <FormMessage />
            </FormItem>
          )}
        />
      </PipelineFormSubsection>
    </PipelineFormSection>
  )
}
