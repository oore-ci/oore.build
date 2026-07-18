import { useFormContext } from 'react-hook-form'
import { DynamicLucideIcon } from '@/components/ui/dynamic-lucide-icon'
import {
  CircleAlert as AlertCircleIcon,
} from 'lucide-react'

import type { PipelineFormValues } from '@/lib/pipeline-schema'
import { TRIGGER_EVENTS } from '@/lib/pipeline-schema'
import { PipelineFormSectionHeader } from '@/components/pipeline-form-section-header'
import { Alert, AlertDescription } from '@/components/ui/alert'
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
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'

const CONFIG_SOURCES: Record<string, string> = {
  auto: 'Use repo config if found (.oore.yaml, .oore.yml)',
  explicit: 'Use a specific config file path',
}

export function PipelineIdentityAndConfigSection({
  configMode,
  onOpenChange,
  open,
  platforms,
  previewDefaults,
  repositoryWorkflow,
}: {
  configMode: PipelineFormValues['config_mode']
  onOpenChange: (open: boolean) => void
  open: boolean
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

      <Collapsible open={open} onOpenChange={onOpenChange}>
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
                open={open}
              />
            </CardHeader>
          </CollapsibleTrigger>
          <CollapsibleContent>
            <CardContent
              className={
                repositoryWorkflow
                  ? 'space-y-4 [&>:not(.repository-workflow-summary)]:hidden'
                  : 'space-y-4'
              }
            >
              {repositoryWorkflow ? (
                <div className="repository-workflow-summary">
                  {repositoryWorkflow}
                </div>
              ) : null}
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
                        <SelectTrigger className="w-full">
                          <SelectValue placeholder="Choose source" />
                        </SelectTrigger>
                      </FormControl>
                      <SelectContent>
                        {Object.entries(CONFIG_SOURCES).map(([key, value]) => (
                          <SelectItem key={key} value={key}>
                            {value}
                          </SelectItem>
                        ))}
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
                <FormLabel>Build for platforms</FormLabel>
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
                      If set, runner executes Flutter commands using{' '}
                      <span className="font-mono">fvm use &lt;version&gt;</span>
                      . If unset, oore will auto-read{' '}
                      <span className="font-mono">.fvmrc</span> from the repo
                      when present.
                    </p>
                    <FormMessage />
                  </FormItem>
                )}
              />

              {previewDefaults.length > 0 ? (
                <div className="space-y-1 border p-3">
                  <p className="text-xs font-medium text-muted-foreground">
                    Default build commands
                  </p>
                  <ul className="list-disc space-y-0.5 pl-4 text-xs text-muted-foreground">
                    {previewDefaults.map((command) => (
                      <li key={command} className="font-mono">
                        {command}
                      </li>
                    ))}
                  </ul>
                </div>
              ) : null}
            </CardContent>
          </CollapsibleContent>
        </Card>
      </Collapsible>
    </>
  )
}

export function PipelineTriggersSection({
  cancelPrevious,
  manualOnlyTriggers,
  onCancelPreviousChange,
  onOpenChange,
  onToggleEvent,
  open,
  selectedEvents,
}: {
  cancelPrevious: boolean
  manualOnlyTriggers: boolean
  onCancelPreviousChange: (checked: boolean) => void
  onOpenChange: (open: boolean) => void
  onToggleEvent: (event: string) => void
  open: boolean
  selectedEvents: Array<string>
}) {
  const form = useFormContext<PipelineFormValues>()
  return (
    <Collapsible open={open} onOpenChange={onOpenChange}>
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
              open={open}
            />
          </CardHeader>
        </CollapsibleTrigger>
        <CollapsibleContent>
          <CardContent className="space-y-4">
            {manualOnlyTriggers ? (
              <Alert>
                <DynamicLucideIcon icon={AlertCircleIcon} size={16} />
                <AlertDescription>
                  This repository uses local Git — builds can only be triggered
                  manually from the UI or API. Webhook triggers require a
                  connected GitHub or GitLab source.
                </AlertDescription>
              </Alert>
            ) : (
              <>
                <div className="space-y-2">
                  <FormLabel>Trigger events</FormLabel>
                  <div className="grid grid-cols-1 gap-2 sm:grid-cols-3">
                    {TRIGGER_EVENTS.map((event) => (
                      <label
                        key={event}
                        className="flex items-center gap-2 text-sm"
                      >
                        <Checkbox
                          checked={selectedEvents.includes(event)}
                          onCheckedChange={() => onToggleEvent(event)}
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
                onCheckedChange={(checked) => onCancelPreviousChange(!!checked)}
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
