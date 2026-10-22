import { useFormContext } from 'react-hook-form'

import type { PipelineFormValues } from '@/lib/pipeline-schema'
import {
  PipelineFormSection,
  PipelineFormSubsection,
} from '@/components/pipeline-form-section'
import { SettingsSurfaceSeparator } from '@/components/settings/settings-section'
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
  repositoryWorkflow,
}: {
  configMode: PipelineFormValues['config_mode']
  repositoryWorkflow?: React.ReactNode
}) {
  const form = useFormContext<PipelineFormValues>()
  return (
    <PipelineFormSection
      id="basics"
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
                        placeholder="e.g. ci/oore.yaml"
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
                      placeholder="Empty uses .fvmrc or stable"
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
          </PipelineFormSubsection>
        </>
      )}
    </PipelineFormSection>
  )
}
