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
  WorkflowField,
  WorkflowFileLookup,
  type PipelineWorkflowLookup,
  type PipelineWorkflowSource,
} from '@/components/pipeline-workflow-field'
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'

const CONFIG_SOURCES = {
  auto: 'Use .oore.yaml (or .oore.yml) if the repository has one',
  explicit: 'Use a file at another path',
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
  workflowLookup,
  workflowSource,
}: {
  configMode: PipelineFormValues['config_mode']
  repositoryWorkflow?: React.ReactNode
  /** Reads the config path from the repository for the Workflow field. */
  workflowLookup?: PipelineWorkflowLookup
  /** Shown as a Workflow selector when the file defines workflows. */
  workflowSource?: PipelineWorkflowSource
}) {
  const workflowField =
    workflowSource && workflowSource.workflows.length > 0 ? (
      <WorkflowField source={workflowSource} />
    ) : null
  const form = useFormContext<PipelineFormValues>()
  return (
    <PipelineFormSection id="basics" title="Basics">
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
          hint="Build settings come from the workflow checked into the repository."
        >
          {repositoryWorkflow}
          {workflowField}
        </PipelineFormSubsection>
      ) : (
        <>
          <PipelineFormSubsection title="Platforms">
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

          <PipelineFormSubsection title="Pipeline file">
            <FormField
              control={form.control}
              name="config_mode"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>File</FormLabel>
                  <Select
                    value={field.value}
                    onValueChange={field.onChange}
                    items={CONFIG_SOURCES}
                  >
                    <FormControl>
                      <SelectTrigger
                        className="w-full"
                        aria-label="Pipeline file"
                      >
                        <SelectValue placeholder="Choose a file" />
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
                    <FormLabel>File path</FormLabel>
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

            {workflowLookup ? (
              <WorkflowFileLookup {...workflowLookup} />
            ) : (
              workflowField
            )}

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
                    Only for a new store number on every build. By default the
                    store number is the <span className="font-mono">+N</span> in{' '}
                    <span className="font-mono">pubspec.yaml</span>, bumped for
                    each release. To number every build instead, pass{' '}
                    <span className="font-mono">
                      --build-number=$OORE_VERSION_CODE
                    </span>{' '}
                    to <span className="font-mono">flutter build</span> and set
                    this above the store&apos;s highest number;{' '}
                    <span className="font-mono">OORE_VERSION_CODE</span> is the
                    build number plus this offset.
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
