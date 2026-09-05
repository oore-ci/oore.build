import { useFormContext } from 'react-hook-form'
import { HugeiconsIcon } from '@hugeicons/react'
import { AlertCircleIcon } from '@hugeicons/core-free-icons'

import type { PipelineFormValues } from '@/lib/pipeline-schema'
import { TRIGGER_EVENTS } from '@/lib/pipeline-schema'
import { PipelineFormSectionHeader } from '@/components/pipeline-form-section-header'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Card, CardContent, CardHeader } from '@/components/ui/card'
import { Checkbox } from '@/components/ui/checkbox'
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible'
import {
  FormControl,
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
  explicit: 'Use a specific config file path',
} satisfies Record<string, string>

export function PipelineIdentityAndConfigSection({
  defaultOpen = true,
  configMode,
  platforms,
  previewDefaults,
  repositoryWorkflow,
}: {
  defaultOpen?: boolean
  configMode: PipelineFormValues['config_mode']
  platforms: Array<string>
  previewDefaults: Array<string>
  repositoryWorkflow?: React.ReactNode
}) {
  const form = useFormContext<PipelineFormValues>()
  return (
    <>
      <Card>
        <CardContent>
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
        </CardContent>
      </Card>

      <Collapsible defaultOpen={defaultOpen}>
        <Card>
          <CollapsibleTrigger className="w-full cursor-pointer">
            <CardHeader>
              <PipelineFormSectionHeader
                title="Configuration"
                summary={
                  repositoryWorkflow
                    ? 'Owned by repository'
                    : `${platforms.length} platform${platforms.length !== 1 ? 's' : ''}, ${configMode === 'explicit' ? 'explicit config' : 'auto-detect'}`
                }
              />
            </CardHeader>
          </CollapsibleTrigger>
          <CollapsibleContent>
            <CardContent className="space-y-4">
              {repositoryWorkflow ? (
                repositoryWorkflow
              ) : (
                <>
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
                              {Object.entries(CONFIG_SOURCES).map(
                                ([key, value]) => (
                                  <SelectItem key={key} value={key}>
                                    {value}
                                  </SelectItem>
                                ),
                              )}
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
                            <Input placeholder="ci/oore.yaml" {...field} />
                          </FormControl>
                          <FormMessage />
                        </FormItem>
                      )}
                    />
                  ) : null}

                  <div className="space-y-2">
                    <p className="text-sm font-medium">Build for platforms</p>
                    <div className="grid grid-cols-1 gap-2 sm:grid-cols-3">
                      {(
                        [
                          ['platform_android', 'Android'],
                          ['platform_ios', 'iOS'],
                          ['platform_macos', 'macOS'],
                        ] as const
                      ).map(([name, label]) => (
                        <FormField
                          key={name}
                          control={form.control}
                          name={name}
                          render={({ field }) => (
                            <label className="flex items-center gap-2 text-sm">
                              <Checkbox
                                checked={field.value}
                                onCheckedChange={(checked) =>
                                  field.onChange(!!checked)
                                }
                              />
                              {label}
                            </label>
                          )}
                        />
                      ))}
                    </div>
                    <p className="text-xs text-muted-foreground">
                      Select the platforms you want to build for. You can change
                      this later.
                    </p>
                  </div>

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
                        <p className="text-xs text-muted-foreground">
                          Oore uses <span className="font-mono">.fvmrc</span>{' '}
                          when present, then this value, then managed stable
                          Flutter. The SDK downloads automatically on the first
                          build and is cached for later builds.
                        </p>
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
                </>
              )}
            </CardContent>
          </CollapsibleContent>
        </Card>
      </Collapsible>
    </>
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
    <Collapsible defaultOpen>
      <Card>
        <CollapsibleTrigger className="w-full cursor-pointer">
          <CardHeader>
            <PipelineFormSectionHeader
              title="Triggers"
              summary={
                manualOnlyTriggers
                  ? `manual only, cancel previous: ${cancelPrevious ? 'on' : 'off'}`
                  : `${selectedEvents.length} event${selectedEvents.length !== 1 ? 's' : ''}, cancel previous: ${cancelPrevious ? 'on' : 'off'}`
              }
            />
          </CardHeader>
        </CollapsibleTrigger>
        <CollapsibleContent>
          <CardContent className="space-y-4">
            {manualOnlyTriggers ? (
              <Alert>
                <HugeiconsIcon icon={AlertCircleIcon} size={16} />
                <AlertDescription>
                  This repository uses local Git — builds can only be triggered
                  manually from the UI or API. Webhook triggers require a
                  connected GitHub or GitLab source.
                </AlertDescription>
              </Alert>
            ) : (
              <>
                <div className="space-y-2">
                  <p className="text-sm font-medium">Trigger events</p>
                  <div className="grid grid-cols-1 gap-2 sm:grid-cols-3">
                    {TRIGGER_EVENTS.map((event) => (
                      <label
                        key={event}
                        className="flex items-center gap-2 text-sm"
                      >
                        <Checkbox
                          checked={selectedEvents.includes(event)}
                          onCheckedChange={() => toggleEvent(event)}
                        />
                        {event}
                      </label>
                    ))}
                  </div>
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
                          {...field}
                        />
                      </FormControl>
                      <FormMessage />
                    </FormItem>
                  )}
                />
              </>
            )}

            <label className="flex items-center gap-2 text-sm">
              <Checkbox
                checked={cancelPrevious}
                onCheckedChange={(checked) =>
                  form.setValue('cancel_previous', !!checked, {
                    shouldDirty: true,
                  })
                }
              />
              Cancel previous builds on same branch
            </label>

            <FormField
              control={form.control}
              name="max_concurrent"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>Max concurrent builds (optional)</FormLabel>
                  <FormControl>
                    <Input type="number" min={1} max={100} {...field} />
                  </FormControl>
                  <FormMessage />
                </FormItem>
              )}
            />
          </CardContent>
        </CollapsibleContent>
      </Card>
    </Collapsible>
  )
}
