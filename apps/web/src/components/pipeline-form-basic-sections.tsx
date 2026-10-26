import { useFormContext, useWatch } from 'react-hook-form'

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
import { Text } from '@/components/layout/text'
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

/** The workflows a pipeline's `.oore.yaml` version 2 file defines. */
export interface PipelineWorkflowSource {
  path: string
  workflows: Array<string>
}

// Base UI selects treat an empty value as unselected.
const DEFAULT_WORKFLOW = '__default__'

function WorkflowField({ source }: { source: PipelineWorkflowSource }) {
  const form = useFormContext<PipelineFormValues>()
  const current = useWatch({ control: form.control, name: 'workflow' }) ?? ''
  // Keep a saved workflow the file no longer defines visible and selected.
  const names =
    current && !source.workflows.includes(current)
      ? [...source.workflows, current]
      : source.workflows
  const items = {
    [DEFAULT_WORKFLOW]:
      source.workflows.length === 1
        ? `File default (${source.workflows[0]})`
        : 'Not set',
    ...Object.fromEntries(names.map((name) => [name, name])),
  }
  return (
    <FormField
      control={form.control}
      name="workflow"
      render={({ field }) => (
        <FormItem>
          <FormLabel>Workflow</FormLabel>
          <Select
            value={field.value || DEFAULT_WORKFLOW}
            onValueChange={(value) =>
              field.onChange(value === DEFAULT_WORKFLOW ? '' : value)
            }
            items={items}
          >
            <FormControl>
              <SelectTrigger className="w-full" aria-label="Workflow">
                <SelectValue />
              </SelectTrigger>
            </FormControl>
            <SelectContent>
              <SelectGroup>
                {Object.entries(items).map(([value, label]) => (
                  <SelectItem key={value} value={value}>
                    {label}
                  </SelectItem>
                ))}
              </SelectGroup>
            </SelectContent>
          </Select>
          <FormDescription>
            {source.workflows.length > 1 ? (
              <>
                <span className="font-mono">{source.path}</span> defines several
                workflows; this pipeline builds the one picked here.
              </>
            ) : (
              <>
                This pipeline builds the workflow in{' '}
                <span className="font-mono">{source.path}</span>.
              </>
            )}{' '}
            When that workflow sets <span className="font-mono">on</span>, it
            replaces the triggers below for pushes, pull requests and tags.
          </FormDescription>
          {current && !source.workflows.includes(current) ? (
            <Text variant="caption" className="text-destructive">
              {source.path} no longer defines {current}; builds will fail until
              you pick another.
            </Text>
          ) : null}
          <FormMessage />
        </FormItem>
      )}
    />
  )
}

/**
 * Basics: what the pipeline is called, what it builds, and where its config
 * comes from. When a repository workflow owns execution, the workflow summary
 * replaces the config fields.
 */
export function PipelineBasicsSection({
  configMode,
  repositoryWorkflow,
  workflowSource,
}: {
  configMode: PipelineFormValues['config_mode']
  repositoryWorkflow?: React.ReactNode
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

          <PipelineFormSubsection title="Configuration">
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

            {workflowField}

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
