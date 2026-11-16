import { useState } from 'react'
import { useForm, useWatch } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import * as z from 'zod'

import type { ProjectEnvironment } from '@oore/client/models'
import { getApiErrorMessage } from '@/lib/api-client/api-error'
import {
  approverMode,
  approversInput,
  ENVIRONMENT_KEY_MAX_LENGTH,
  ENVIRONMENT_KEY_PATTERN,
  ENVIRONMENT_NAME_MAX_LENGTH,
  formatRolloutSteps,
  parseRolloutSteps,
  playTrackStagesRollouts,
} from '@/lib/environments'
import { stackVariants } from '@/lib/recipes'
import { RELEASE_STORE_LABEL } from '@/lib/releases'
import { isPlayTrack, PLAY_TRACKS } from '@/lib/store-destination'
import { toast } from '@/lib/toast'
import {
  useCreateEnvironment,
  useUpdateEnvironment,
} from '@/hooks/use-environments'
import { useProjectMembers } from '@/hooks/use-projects'
import { Stack } from '@/components/layout/stack'
import { Text } from '@/components/layout/text'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import {
  Form,
  FormControl,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
} from '@/components/ui/form'
import { Input } from '@/components/ui/input'
import {
  Item,
  ItemContent,
  ItemDescription,
  ItemMedia,
  ItemTitle,
} from '@/components/ui/item'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Spinner } from '@/components/ui/spinner'
import { Switch } from '@/components/ui/switch'

const DEFAULT_STEPS = [0.01, 0.05, 0.2, 0.5, 1]
/** Apple's phased release schedule, as rollout steps. */
const APPLE_PHASED_STEPS = [0.01, 0.02, 0.05, 0.1, 0.2, 0.5, 1]
const APP_STORE_TRACK = 'app_store'

/** The same track list as the pipeline's Google Play settings. */
const PLAY_TRACK_CHOICES = {
  ...Object.fromEntries(
    Object.entries(PLAY_TRACKS).map(([value, track]) => [value, track.label]),
  ),
  custom: 'Custom track',
}

const APP_STORE_TRACK_CHOICES = {
  [APP_STORE_TRACK]: 'App Store version',
  testflight: 'TestFlight group',
}

/** Which list entry a stored track is: a named track, or the free-text one. */
function trackChoice(store: string, track: string, typing: boolean) {
  const value = track.trim()
  if (store === 'google_play') {
    if (typing) return 'custom'
    if (!value) return null
    return isPlayTrack(value) ? value : 'custom'
  }
  if (typing) return 'testflight'
  if (!value) return null
  return value === APP_STORE_TRACK ? APP_STORE_TRACK : 'testflight'
}

function trackHelp(store: string, track: string) {
  const value = track.trim()
  if (store === 'google_play') {
    if (!value) return 'Where on Google Play this track releases to.'
    return isPlayTrack(value)
      ? PLAY_TRACKS[value].effect
      : 'A custom closed testing track you created in Play Console, by its exact name.'
  }
  if (value === APP_STORE_TRACK) {
    return 'The App Store version people download once Apple approves it.'
  }
  return value
    ? 'A TestFlight group, by its name in App Store Connect.'
    : 'The App Store version, or a TestFlight group.'
}

const APPROVER_MODE_LABEL = {
  maintainer: 'Maintainers',
  developer: 'Developers and maintainers',
  named: 'Only named approvers',
}

const environmentFormSchema = z
  .object({
    key: z
      .string()
      .trim()
      .min(1, 'Enter a key.')
      .max(
        ENVIRONMENT_KEY_MAX_LENGTH,
        `Keys can be at most ${ENVIRONMENT_KEY_MAX_LENGTH} characters.`,
      )
      .regex(
        ENVIRONMENT_KEY_PATTERN,
        'Use lowercase letters, digits and dashes, starting with a letter.',
      ),
    name: z
      .string()
      .trim()
      .min(1, 'Enter a name.')
      .max(
        ENVIRONMENT_NAME_MAX_LENGTH,
        `Names can be at most ${ENVIRONMENT_NAME_MAX_LENGTH} characters.`,
      ),
    store: z.enum(['google_play', 'app_store']),
    track: z.string().trim().min(1, 'Choose where this track goes.'),
    approval_required: z.boolean(),
    approver_mode: z.enum(['maintainer', 'developer', 'named']),
    approver_user_ids: z.array(z.string()),
    phased_release: z.boolean(),
    submit_for_review: z.boolean(),
    steps: z.string(),
    interval_hours: z
      .number({ error: 'Enter a number of hours.' })
      .positive('Use more than 0 hours.'),
    crash_rate_percent: z
      .number()
      .positive('Use more than 0%.')
      .max(100, 'Use at most 100%.')
      .optional(),
  })
  .superRefine((values, context) => {
    if (
      values.approval_required &&
      values.approver_mode === 'named' &&
      values.approver_user_ids.length === 0
    ) {
      context.addIssue({
        code: 'custom',
        path: ['approver_user_ids'],
        message: 'Choose at least one approver.',
      })
    }
    const parsed = parseRolloutSteps(values.steps)
    if ('error' in parsed) {
      context.addIssue({
        code: 'custom',
        path: ['steps'],
        message: parsed.error,
      })
    }
  })

type EnvironmentFormValues = z.infer<typeof environmentFormSchema>

function defaultValues(
  environment: ProjectEnvironment | undefined,
  store: ProjectEnvironment['store'] | undefined,
): EnvironmentFormValues {
  const policy = environment?.rollout_policy
  return {
    key: environment?.key ?? '',
    name: environment?.name ?? '',
    store: environment?.store ?? store ?? 'google_play',
    track: environment?.track ?? '',
    approval_required: environment?.approval_required ?? false,
    approver_mode: approverMode(environment?.approvers),
    approver_user_ids: (environment?.approvers?.users ?? []).map(
      (user) => user.user_id,
    ),
    phased_release: (policy?.steps ?? DEFAULT_STEPS).length > 1,
    submit_for_review: environment?.submit_for_review ?? false,
    steps: formatRolloutSteps(policy?.steps ?? DEFAULT_STEPS),
    interval_hours: policy?.interval_hours ?? 24,
    crash_rate_percent:
      policy?.crash_rate_threshold == null
        ? undefined
        : policy.crash_rate_threshold * 100,
  }
}

function numberOrUndefined(value: string) {
  return value === '' ? undefined : Number(value)
}

/**
 * Add a track (an environment in the API), or edit one when `environment`
 * is set. A `store` fixes the store, as the Stores tab opens it per store.
 */
export function EnvironmentDialog({
  environment,
  onOpenChange,
  open,
  projectId,
  store: fixedStore,
}: {
  environment?: ProjectEnvironment
  onOpenChange: (open: boolean) => void
  open: boolean
  projectId: string
  store?: ProjectEnvironment['store']
}) {
  const isEdit = environment !== undefined
  // The daemon keeps the track of an environment that has releases.
  const trackLocked = environment?.latest_release != null
  const createMutation = useCreateEnvironment(projectId)
  const updateMutation = useUpdateEnvironment(projectId)
  const mutation = isEdit ? updateMutation : createMutation
  const form = useForm<EnvironmentFormValues>({
    resolver: zodResolver(environmentFormSchema),
    values: defaultValues(environment, fixedStore),
  })
  // Picking "Custom track" or "TestFlight group" shows a text field.
  const [typingTrack, setTypingTrack] = useState(false)
  const [store, track, approvalRequired, approverModeValue] = useWatch({
    control: form.control,
    name: ['store', 'track', 'approval_required', 'approver_mode'],
  })
  const membersQuery = useProjectMembers(projectId, open)
  // Viewers cannot approve, so they are not offered.
  const approverCandidates = (membersQuery.data?.members ?? []).filter(
    (member) => member.role !== 'viewer',
  )
  const playTrack = store === 'google_play' && track.trim() !== ''
  const staged = playTrack && playTrackStagesRollouts(track)
  const appStoreVersion =
    store === 'app_store' && track.trim() === APP_STORE_TRACK

  /** Submit for review only applies to the App Store version's track. */
  function clearSubmitUnlessAppStoreVersion(
    nextStore: string,
    nextTrack: string,
  ) {
    if (nextStore !== 'app_store' || nextTrack.trim() !== APP_STORE_TRACK) {
      form.setValue('submit_for_review', false, { shouldDirty: true })
    }
  }

  function handleOpenChange(next: boolean) {
    if (!next) {
      form.reset()
      mutation.reset()
      setTypingTrack(false)
    }
    onOpenChange(next)
  }

  function onSubmit(values: EnvironmentFormValues) {
    const parsed = parseRolloutSteps(values.steps)
    if ('error' in parsed) return
    const versionTrack =
      values.store === 'app_store' && values.track.trim() === APP_STORE_TRACK
    // App Store releases follow Apple's schedule: phased, or all at once.
    // TestFlight, and Play tracks other than production and open testing,
    // have no staged rollout.
    const steps =
      values.store === 'google_play'
        ? playTrackStagesRollouts(values.track)
          ? parsed.steps
          : [1]
        : versionTrack && values.phased_release
          ? APPLE_PHASED_STEPS
          : [1]
    const submit_for_review = versionTrack && values.submit_for_review
    // Approvers only matter with approval on; otherwise keep what is stored.
    const approvers = values.approval_required
      ? approversInput(values.approver_mode, values.approver_user_ids)
      : undefined
    const rollout_policy = {
      steps,
      interval_hours: values.interval_hours,
      crash_rate_threshold:
        values.crash_rate_percent == null
          ? null
          : values.crash_rate_percent / 100,
      approval: 'none' as const,
    }
    const onError = (err: Error) =>
      form.setError('root', {
        message: getApiErrorMessage(err, {
          environment_exists:
            'Another track already uses this key, or this store track.',
        }),
      })
    if (environment) {
      updateMutation.mutate(
        {
          environmentId: environment.id,
          body: {
            name: values.name,
            track: values.track,
            approval_required: values.approval_required,
            approvers,
            rollout_policy,
            submit_for_review,
          },
        },
        {
          onSuccess: (saved) => {
            toast.success(`Saved ${saved.name}`)
            handleOpenChange(false)
          },
          onError,
        },
      )
    } else {
      createMutation.mutate(
        {
          key: values.key,
          name: values.name,
          store: values.store,
          track: values.track,
          approval_required: values.approval_required,
          approvers,
          rollout_policy,
          submit_for_review,
        },
        {
          onSuccess: (saved) => {
            toast.success(`Added ${saved.name}`)
            handleOpenChange(false)
          },
          onError,
        },
      )
    }
  }

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>
            {isEdit
              ? `Edit ${environment.name}`
              : fixedStore
                ? `Add ${RELEASE_STORE_LABEL[fixedStore]} track`
                : 'Add track'}
          </DialogTitle>
        </DialogHeader>

        <Form {...form}>
          <form
            onSubmit={(event) => void form.handleSubmit(onSubmit)(event)}
            className={stackVariants({ gap: 4 })}
          >
            <div className="grid gap-4 sm:grid-cols-2">
              <FormField
                control={form.control}
                name="name"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel>Name</FormLabel>
                    <FormControl>
                      <Input
                        autoFocus
                        autoComplete="off"
                        placeholder="Beta"
                        {...field}
                      />
                    </FormControl>
                    <FormMessage />
                  </FormItem>
                )}
              />
              <FormField
                control={form.control}
                name="key"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel>Key</FormLabel>
                    <FormControl>
                      <Input
                        autoComplete="off"
                        spellCheck={false}
                        className="font-mono"
                        placeholder="beta"
                        disabled={isEdit}
                        {...field}
                      />
                    </FormControl>
                    <FormMessage />
                  </FormItem>
                )}
              />
            </div>

            <div className="grid gap-4 sm:grid-cols-2">
              {fixedStore ? null : (
                <FormField
                  control={form.control}
                  name="store"
                  render={({ field }) => (
                    <FormItem>
                      <FormLabel>Store</FormLabel>
                      <Select
                        value={field.value}
                        onValueChange={(next) => {
                          if (next === 'google_play' || next === 'app_store') {
                            field.onChange(next)
                            setTypingTrack(false)
                            form.setValue('track', '', { shouldDirty: true })
                            clearSubmitUnlessAppStoreVersion(next, '')
                          }
                        }}
                        items={RELEASE_STORE_LABEL}
                        disabled={isEdit}
                      >
                        <FormControl>
                          <SelectTrigger className="w-full">
                            <SelectValue />
                          </SelectTrigger>
                        </FormControl>
                        <SelectContent>
                          {Object.entries(RELEASE_STORE_LABEL).map(
                            ([value, label]) => (
                              <SelectItem key={value} value={value}>
                                {label}
                              </SelectItem>
                            ),
                          )}
                        </SelectContent>
                      </Select>
                      <FormMessage />
                    </FormItem>
                  )}
                />
              )}
              <FormField
                control={form.control}
                name="track"
                render={({ field }) => {
                  const choice = trackChoice(store, field.value, typingTrack)
                  const choices =
                    store === 'google_play'
                      ? PLAY_TRACK_CHOICES
                      : APP_STORE_TRACK_CHOICES
                  const freeText =
                    choice === 'custom' || choice === 'testflight'
                  return (
                    <FormItem>
                      <FormLabel>
                        {store === 'google_play' ? 'Play track' : 'Goes to'}
                      </FormLabel>
                      <Select
                        value={choice}
                        items={choices}
                        disabled={trackLocked}
                        onValueChange={(next) => {
                          if (!next) return
                          const typing =
                            next === 'custom' || next === 'testflight'
                          setTypingTrack(typing)
                          const value = typing ? '' : next
                          form.setValue('track', value, {
                            shouldDirty: true,
                            shouldValidate: form.formState.isSubmitted,
                          })
                          clearSubmitUnlessAppStoreVersion(store, value)
                        }}
                      >
                        <FormControl>
                          <SelectTrigger className="w-full">
                            <SelectValue
                              placeholder={
                                store === 'google_play'
                                  ? 'Choose a Play track'
                                  : 'Choose where'
                              }
                            />
                          </SelectTrigger>
                        </FormControl>
                        <SelectContent>
                          {Object.entries(choices).map(([value, label]) => (
                            <SelectItem key={value} value={value}>
                              {label}
                            </SelectItem>
                          ))}
                        </SelectContent>
                      </Select>
                      {freeText ? (
                        <FormControl>
                          <Input
                            autoComplete="off"
                            spellCheck={false}
                            className="font-mono"
                            aria-label={
                              store === 'google_play'
                                ? 'Custom track name'
                                : 'TestFlight group name'
                            }
                            placeholder={
                              store === 'google_play'
                                ? 'qa-team'
                                : 'Internal QA'
                            }
                            disabled={trackLocked}
                            {...field}
                            onChange={(event) => {
                              field.onChange(event)
                              clearSubmitUnlessAppStoreVersion(
                                store,
                                event.target.value,
                              )
                            }}
                          />
                        </FormControl>
                      ) : null}
                      <FormMessage />
                    </FormItem>
                  )
                }}
              />
            </div>
            <Text className="-mt-2">
              {trackLocked
                ? 'This track has releases, so where it goes is fixed. Add a new track to go somewhere else.'
                : trackHelp(store, track)}
            </Text>

            <FormField
              control={form.control}
              name="approval_required"
              render={({ field }) => (
                <FormItem className="flex flex-row items-start justify-between gap-4 rounded-lg border p-3">
                  <Stack gap={1}>
                    <FormLabel
                      hint={
                        'Developers can ask to promote; an approver approves before anything changes in the store.'
                      }
                    >
                      Require approval
                    </FormLabel>
                  </Stack>
                  <FormControl>
                    <Switch
                      checked={field.value}
                      onCheckedChange={field.onChange}
                    />
                  </FormControl>
                </FormItem>
              )}
            />

            {approvalRequired ? (
              <Stack gap={4} className="rounded-lg border p-3">
                <FormField
                  control={form.control}
                  name="approver_mode"
                  render={({ field }) => (
                    <FormItem>
                      <FormLabel>Who can approve</FormLabel>
                      <Select
                        value={field.value}
                        onValueChange={(next) => {
                          if (
                            next === 'maintainer' ||
                            next === 'developer' ||
                            next === 'named'
                          ) {
                            field.onChange(next)
                          }
                        }}
                        items={APPROVER_MODE_LABEL}
                      >
                        <FormControl>
                          <SelectTrigger className="w-full">
                            <SelectValue />
                          </SelectTrigger>
                        </FormControl>
                        <SelectContent>
                          {Object.entries(APPROVER_MODE_LABEL).map(
                            ([value, label]) => (
                              <SelectItem key={value} value={value}>
                                {label}
                              </SelectItem>
                            ),
                          )}
                        </SelectContent>
                      </Select>
                      <FormMessage />
                    </FormItem>
                  )}
                />
                <FormField
                  control={form.control}
                  name="approver_user_ids"
                  render={({ field }) => (
                    <FormItem>
                      <FormLabel
                        hint={
                          approverModeValue === 'named'
                            ? 'Only these members can approve. Instance admins can too.'
                            : 'These members can approve as well, whatever their role.'
                        }
                      >
                        Named approvers
                      </FormLabel>
                      {membersQuery.isLoading ? (
                        <Spinner />
                      ) : approverCandidates.length === 0 ? (
                        <Text render={<span />}>
                          No developers or maintainers in this app yet.
                        </Text>
                      ) : (
                        <div
                          role="group"
                          aria-label="Named approvers"
                          className="grid gap-2"
                        >
                          {approverCandidates.map((member) => (
                            <Item
                              key={member.user_id}
                              render={<label />}
                              variant="outline"
                              size="sm"
                              className="has-data-checked:border-primary has-data-checked:bg-accent"
                            >
                              <ItemMedia>
                                <Checkbox
                                  checked={field.value.includes(member.user_id)}
                                  onCheckedChange={(checked) =>
                                    field.onChange(
                                      checked
                                        ? [...field.value, member.user_id]
                                        : field.value.filter(
                                            (id) => id !== member.user_id,
                                          ),
                                    )
                                  }
                                />
                              </ItemMedia>
                              <ItemContent>
                                <ItemTitle>
                                  {member.user_display_name ??
                                    member.user_email}
                                </ItemTitle>
                                <ItemDescription>
                                  {member.user_display_name
                                    ? `${member.user_email} · `
                                    : ''}
                                  {member.role === 'maintainer'
                                    ? 'Maintainer'
                                    : 'Developer'}
                                </ItemDescription>
                              </ItemContent>
                            </Item>
                          ))}
                        </div>
                      )}
                      <FormMessage />
                    </FormItem>
                  )}
                />
              </Stack>
            ) : null}

            {appStoreVersion ? (
              <>
                <FormField
                  control={form.control}
                  name="phased_release"
                  render={({ field }) => (
                    <FormItem className="flex flex-row items-start justify-between gap-4 rounded-lg border p-3">
                      <Stack gap={1}>
                        <FormLabel
                          hint={
                            "Release over 7 days on Apple's schedule. Off releases to everyone once Apple approves it."
                          }
                        >
                          Phased release
                        </FormLabel>
                      </Stack>
                      <FormControl>
                        <Switch
                          checked={field.value}
                          onCheckedChange={field.onChange}
                        />
                      </FormControl>
                    </FormItem>
                  )}
                />
                <FormField
                  control={form.control}
                  name="submit_for_review"
                  render={({ field }) => (
                    <FormItem className="flex flex-row items-start justify-between gap-4 rounded-lg border p-3">
                      <Stack gap={1}>
                        <FormLabel
                          hint={
                            'Submit the version for App Review once a promotion attaches the build. Off leaves it for you to submit in App Store Connect.'
                          }
                        >
                          Submit for review
                        </FormLabel>
                      </Stack>
                      <FormControl>
                        <Switch
                          checked={field.value}
                          onCheckedChange={field.onChange}
                        />
                      </FormControl>
                    </FormItem>
                  )}
                />
              </>
            ) : null}

            {playTrack && !staged ? (
              <Text>
                Google Play releases to everyone at once on this track. Staged
                rollouts run only on production and open testing.
              </Text>
            ) : null}

            {staged ? (
              <>
                <FormField
                  control={form.control}
                  name="steps"
                  render={({ field }) => (
                    <FormItem>
                      <FormLabel
                        hint={
                          'A promoted release starts at the first step and ends at 100.'
                        }
                      >
                        Rollout steps (%)
                      </FormLabel>
                      <FormControl>
                        <Input
                          autoComplete="off"
                          className="font-mono"
                          placeholder="1, 5, 20, 50, 100"
                          {...field}
                        />
                      </FormControl>
                      <FormMessage />
                    </FormItem>
                  )}
                />
                <div className="grid gap-4 sm:grid-cols-2">
                  <FormField
                    control={form.control}
                    name="interval_hours"
                    render={({ field }) => (
                      <FormItem>
                        <FormLabel>Hours between steps</FormLabel>
                        <FormControl>
                          <Input
                            type="number"
                            inputMode="decimal"
                            min={0}
                            step="any"
                            {...field}
                            value={field.value}
                            onChange={(event) =>
                              field.onChange(
                                numberOrUndefined(event.target.value),
                              )
                            }
                          />
                        </FormControl>
                        <FormMessage />
                      </FormItem>
                    )}
                  />
                  <FormField
                    control={form.control}
                    name="crash_rate_percent"
                    render={({ field }) => (
                      <FormItem>
                        <FormLabel>Halt at crash rate (%)</FormLabel>
                        <FormControl>
                          <Input
                            type="number"
                            inputMode="decimal"
                            min={0}
                            max={100}
                            step="any"
                            placeholder="Off"
                            {...field}
                            value={field.value ?? ''}
                            onChange={(event) =>
                              field.onChange(
                                numberOrUndefined(event.target.value),
                              )
                            }
                          />
                        </FormControl>
                        <FormMessage />
                      </FormItem>
                    )}
                  />
                </div>
              </>
            ) : null}

            {form.formState.errors.root ? (
              <p role="alert" className="text-sm text-destructive">
                {form.formState.errors.root.message}
              </p>
            ) : null}

            <DialogFooter>
              <Button
                type="button"
                variant="outline"
                onClick={() => handleOpenChange(false)}
              >
                Cancel
              </Button>
              <Button
                type="submit"
                disabled={
                  mutation.isPending || (isEdit && !form.formState.isDirty)
                }
              >
                {mutation.isPending ? (
                  <Spinner data-icon="inline-start" />
                ) : null}
                {isEdit ? 'Save changes' : 'Add track'}
              </Button>
            </DialogFooter>
          </form>
        </Form>
      </DialogContent>
    </Dialog>
  )
}
