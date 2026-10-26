import { useFieldArray, useFormContext } from 'react-hook-form'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  Add01Icon,
  AlertCircleIcon,
  Delete02Icon,
} from '@hugeicons/core-free-icons'

import type { PipelineFormValues } from '@/lib/pipeline-schema'
import {
  APPROVAL_EVENTS,
  MAX_SCHEDULES,
  MAX_TIMEOUT_MINUTES,
  TRIGGER_EVENTS,
} from '@/lib/pipeline-schema'
import { pathsApply, tagsApply } from '@/lib/pipeline-form-utils'
import { formatEnumLabel } from '@/lib/format-utils'
import {
  PipelineFormSection,
  PipelineFormSubsection,
} from '@/components/pipeline-form-section'
import { SettingsSurfaceSeparator } from '@/components/settings/settings-section'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
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
import { Textarea } from '@/components/ui/textarea'
import { Stack } from '@/components/layout/stack'
import { Text } from '@/components/layout/text'

/** Local repositories have no webhook events; only manual and scheduled starts exist. */
const LOCAL_APPROVAL_EVENTS = ['manual', 'schedule'] as const

/**
 * Triggers: what starts a build on its own (events, changed paths,
 * schedules), the limits every build runs under, and whether a maintainer
 * must approve it first.
 */
export function PipelineTriggersSection({
  canChangeApproval,
  manualOnlyTriggers,
}: {
  canChangeApproval: boolean
  manualOnlyTriggers: boolean
}) {
  const form = useFormContext<PipelineFormValues>()
  const selectedEvents = form.watch('trigger_events')
  const cancelPrevious = form.watch('cancel_previous')
  const approvalRequired = form.watch('approval_required')
  const approvalEvents = form.watch('approval_events')
  const schedules = useFieldArray({ control: form.control, name: 'schedules' })
  const approvalOptions = manualOnlyTriggers
    ? LOCAL_APPROVAL_EVENTS
    : APPROVAL_EVENTS

  function toggleEvent(event: (typeof TRIGGER_EVENTS)[number]) {
    form.setValue(
      'trigger_events',
      selectedEvents.includes(event)
        ? selectedEvents.filter((entry) => entry !== event)
        : [...selectedEvents, event],
      { shouldDirty: true },
    )
  }

  function toggleApprovalEvent(event: string) {
    form.setValue(
      'approval_events',
      approvalEvents.includes(event)
        ? approvalEvents.filter((entry) => entry !== event)
        : [...approvalEvents, event],
      { shouldDirty: true },
    )
  }

  return (
    <PipelineFormSection
      id="triggers"
      title="Triggers"
      hint="You can always run the pipeline by hand, whatever is set here."
    >
      {manualOnlyTriggers ? (
        <Alert role="note">
          <HugeiconsIcon icon={AlertCircleIcon} size={16} />
          <AlertDescription>
            This repository uses local Git, so builds start from the UI, the API
            or a schedule. Webhook triggers need a connected GitHub or GitLab
            source.
          </AlertDescription>
        </Alert>
      ) : (
        <>
          <PipelineFormSubsection title="Events">
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
                    Comma-separated globs for push and pull request events.
                    Leave empty to build every branch.
                  </FormDescription>
                  <FormMessage />
                </FormItem>
              )}
            />
            {tagsApply(selectedEvents) ? (
              <FormField
                control={form.control}
                name="tags"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel>Tag patterns (optional)</FormLabel>
                    <FormControl>
                      <Input
                        placeholder="v*, release-*"
                        className="font-mono"
                        {...field}
                      />
                    </FormControl>
                    <FormDescription>
                      Comma-separated globs for tag pushes. With branch patterns
                      set and no tag patterns, tag pushes do not build.
                    </FormDescription>
                    <FormMessage />
                  </FormItem>
                )}
              />
            ) : null}
          </PipelineFormSubsection>

          {pathsApply(selectedEvents) ? (
            <>
              <SettingsSurfaceSeparator />
              <PipelineFormSubsection
                title="Changed paths"
                hint="Only pushes and pull requests that touch matching files build. Tag pushes always build."
              >
                <div className="grid gap-4 sm:grid-cols-2">
                  <PathPatternsField
                    name="paths"
                    label="Only these paths (optional)"
                    placeholder={'lib/\nandroid/**\npubspec.yaml'}
                    description="Build when any changed file matches."
                  />
                  <PathPatternsField
                    name="paths_ignore"
                    label="Ignore these paths (optional)"
                    placeholder={'docs/**\n*.md'}
                    description="Skip when every changed file matches."
                  />
                </div>
                <Text variant="muted">
                  One glob per line, relative to the repository root.{' '}
                  <span className="font-mono">*</span> stays within one folder,{' '}
                  <span className="font-mono">**</span> spans folders, and a
                  pattern ending in <span className="font-mono">/</span> matches
                  everything below it. When Oore cannot tell which files
                  changed, it builds.
                </Text>
              </PipelineFormSubsection>
            </>
          ) : null}
        </>
      )}

      <SettingsSurfaceSeparator />

      <PipelineFormSubsection
        title="Schedules"
        hint="Cron schedules are in UTC. Each one builds its branch at the current commit."
        actions={
          <Button
            type="button"
            variant="outline"
            size="sm"
            disabled={schedules.fields.length >= MAX_SCHEDULES}
            onClick={() => schedules.append({ cron: '', branch: '' })}
          >
            <HugeiconsIcon icon={Add01Icon} />
            Add schedule
          </Button>
        }
      >
        {schedules.fields.length === 0 ? (
          <Text variant="muted">
            No schedules. Builds start from events or by hand.
          </Text>
        ) : (
          <Stack gap={3}>
            {schedules.fields.map((schedule, index) => (
              <div
                key={schedule.id}
                className="grid gap-2 sm:grid-cols-[minmax(0,1fr)_minmax(0,1fr)_auto] sm:items-start"
              >
                <FormField
                  control={form.control}
                  name={`schedules.${index}.cron`}
                  render={({ field }) => (
                    <FormItem>
                      <FormLabel
                        className={index === 0 ? undefined : 'sr-only'}
                      >
                        Cron expression
                      </FormLabel>
                      <FormControl>
                        <Input
                          placeholder="0 2 * * 1-5"
                          className="font-mono"
                          aria-label={`Schedule ${index + 1} cron expression`}
                          {...field}
                        />
                      </FormControl>
                      <FormMessage />
                    </FormItem>
                  )}
                />
                <FormField
                  control={form.control}
                  name={`schedules.${index}.branch`}
                  render={({ field }) => (
                    <FormItem>
                      <FormLabel
                        className={index === 0 ? undefined : 'sr-only'}
                      >
                        Branch (optional)
                      </FormLabel>
                      <FormControl>
                        <Input
                          placeholder="Default branch"
                          className="font-mono"
                          aria-label={`Schedule ${index + 1} branch`}
                          {...field}
                        />
                      </FormControl>
                      <FormMessage />
                    </FormItem>
                  )}
                />
                <Button
                  type="button"
                  variant="ghost"
                  size="icon"
                  className={index === 0 ? 'sm:mt-6' : undefined}
                  aria-label={`Remove schedule ${index + 1}`}
                  onClick={() => schedules.remove(index)}
                >
                  <HugeiconsIcon icon={Delete02Icon} />
                </Button>
              </div>
            ))}
          </Stack>
        )}
        <Text variant="muted">
          Five fields (minute, hour, day of month, month, weekday) or{' '}
          <span className="font-mono">@hourly</span>,{' '}
          <span className="font-mono">@daily</span>,{' '}
          <span className="font-mono">@weekly</span>,{' '}
          <span className="font-mono">@monthly</span>. Lists, ranges and steps
          work: <span className="font-mono">1,15</span>,{' '}
          <span className="font-mono">9-17</span>,{' '}
          <span className="font-mono">*/15</span>. Up to {MAX_SCHEDULES}{' '}
          schedules.
        </Text>
        {form.formState.errors.schedules?.root?.message ? (
          <p className="text-sm text-destructive">
            {form.formState.errors.schedules.root.message}
          </p>
        ) : null}
      </PipelineFormSubsection>

      <SettingsSurfaceSeparator />

      <PipelineFormSubsection title="Limits">
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

        <div className="grid gap-4 sm:grid-cols-2">
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
                    placeholder="No limit"
                    {...field}
                  />
                </FormControl>
                <FormDescription>
                  Builds over the limit wait in the queue and start in order as
                  running builds finish.
                </FormDescription>
                <FormMessage />
              </FormItem>
            )}
          />
          <FormField
            control={form.control}
            name="timeout_minutes"
            render={({ field }) => (
              <FormItem>
                <FormLabel>Timeout (minutes)</FormLabel>
                <FormControl>
                  <Input
                    type="number"
                    min={1}
                    max={MAX_TIMEOUT_MINUTES}
                    {...field}
                  />
                </FormControl>
                <FormDescription>
                  A build still running after this is marked timed out and
                  stopped. Up to 24 hours.
                </FormDescription>
                <FormMessage />
              </FormItem>
            )}
          />
        </div>
      </PipelineFormSubsection>

      <SettingsSurfaceSeparator />

      <PipelineFormSubsection
        title="Approval"
        hint={
          canChangeApproval
            ? 'Hold builds until a project maintainer approves them.'
            : 'Only project maintainers can change which builds wait for approval.'
        }
      >
        <label className="flex items-center gap-2 text-sm">
          <Checkbox
            checked={approvalRequired}
            disabled={!canChangeApproval}
            onCheckedChange={(checked) =>
              form.setValue('approval_required', !!checked, {
                shouldDirty: true,
              })
            }
          />
          Builds wait for a maintainer before a runner picks them up
        </label>
        {approvalRequired ? (
          <Stack gap={2}>
            <p className="text-sm font-medium">Which builds</p>
            <div className="grid grid-cols-1 gap-2 sm:grid-cols-3">
              {approvalOptions.map((event) => (
                <label key={event} className="flex items-center gap-2 text-sm">
                  <Checkbox
                    checked={approvalEvents.includes(event)}
                    disabled={!canChangeApproval}
                    onCheckedChange={() => toggleApprovalEvent(event)}
                  />
                  {formatEnumLabel(event)}
                </label>
              ))}
            </div>
            <Text variant="muted">
              {approvalEvents.length === 0
                ? 'Nothing selected, so every build waits. A re-run of a held build waits again.'
                : 'Builds started any other way run without approval.'}
            </Text>
          </Stack>
        ) : null}
      </PipelineFormSubsection>
    </PipelineFormSection>
  )
}

function PathPatternsField({
  description,
  label,
  name,
  placeholder,
}: {
  description: string
  label: string
  name: 'paths' | 'paths_ignore'
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
              rows={3}
              {...field}
            />
          </FormControl>
          <FormDescription>{description}</FormDescription>
          <FormMessage />
        </FormItem>
      )}
    />
  )
}
