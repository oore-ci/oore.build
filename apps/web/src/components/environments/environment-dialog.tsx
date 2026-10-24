import { useForm, useWatch } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import * as z from 'zod'

import type { ProjectEnvironment } from '@oore/client/models'
import { getApiErrorMessage } from '@/lib/api-client/api-error'
import {
  ENVIRONMENT_KEY_MAX_LENGTH,
  ENVIRONMENT_KEY_PATTERN,
  ENVIRONMENT_NAME_MAX_LENGTH,
  formatRolloutSteps,
  parseRolloutSteps,
} from '@/lib/environments'
import { stackVariants } from '@/lib/recipes'
import { RELEASE_STORE_LABEL } from '@/lib/releases'
import { toast } from '@/lib/toast'
import {
  useCreateEnvironment,
  useUpdateEnvironment,
} from '@/hooks/use-environments'
import { Stack } from '@/components/layout/stack'
import { Text } from '@/components/layout/text'
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import {
  Form,
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
    track: z.string().trim().min(1, 'Enter a track.'),
    approval_required: z.boolean(),
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
): EnvironmentFormValues {
  const policy = environment?.rollout_policy
  return {
    key: environment?.key ?? '',
    name: environment?.name ?? '',
    store: environment?.store ?? 'google_play',
    track: environment?.track ?? '',
    approval_required: environment?.approval_required ?? false,
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

/** Add an environment, or edit one when `environment` is set. */
export function EnvironmentDialog({
  environment,
  onOpenChange,
  open,
  projectId,
}: {
  environment?: ProjectEnvironment
  onOpenChange: (open: boolean) => void
  open: boolean
  projectId: string
}) {
  const isEdit = environment !== undefined
  // The daemon keeps the track of an environment that has releases.
  const trackLocked = environment?.latest_release != null
  const createMutation = useCreateEnvironment(projectId)
  const updateMutation = useUpdateEnvironment(projectId)
  const mutation = isEdit ? updateMutation : createMutation
  const form = useForm<EnvironmentFormValues>({
    resolver: zodResolver(environmentFormSchema),
    values: defaultValues(environment),
  })
  const [store, track] = useWatch({
    control: form.control,
    name: ['store', 'track'],
  })
  const staged = store === 'google_play' && track.trim() !== 'internal'
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
    }
    onOpenChange(next)
  }

  function onSubmit(values: EnvironmentFormValues) {
    const parsed = parseRolloutSteps(values.steps)
    if ('error' in parsed) return
    const versionTrack =
      values.store === 'app_store' && values.track.trim() === APP_STORE_TRACK
    // App Store releases follow Apple's schedule: phased, or all at once.
    // TestFlight has no staged rollout.
    const steps =
      values.store === 'google_play'
        ? parsed.steps
        : versionTrack && values.phased_release
          ? APPLE_PHASED_STEPS
          : [1]
    const submit_for_review = versionTrack && values.submit_for_review
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
            'Another environment already uses this key, or this store and track.',
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
            {isEdit ? `Edit ${environment.name}` : 'Add environment'}
          </DialogTitle>
          <DialogDescription>
            Builds are promoted into an environment without being rebuilt.
            Releases uploaded to its track land in it too.
          </DialogDescription>
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
                        placeholder="Production"
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
                        placeholder="production"
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
                          clearSubmitUnlessAppStoreVersion(next, track)
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
              <FormField
                control={form.control}
                name="track"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel>Track</FormLabel>
                    <FormControl>
                      <Input
                        autoComplete="off"
                        spellCheck={false}
                        className="font-mono"
                        placeholder={
                          store === 'google_play' ? 'production' : 'app_store'
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
                    <FormMessage />
                  </FormItem>
                )}
              />
            </div>
            <Text className="-mt-2">
              {trackLocked
                ? 'This environment has releases, so its track is fixed. Add a new environment for another track.'
                : store === 'google_play'
                  ? 'internal, closed, open, production, or a custom track name.'
                  : 'app_store for the App Store version, or a TestFlight group name.'}
            </Text>

            <FormField
              control={form.control}
              name="approval_required"
              render={({ field }) => (
                <FormItem className="flex flex-row items-start justify-between gap-4 rounded-lg border p-3">
                  <Stack gap={1}>
                    <FormLabel>Require approval</FormLabel>
                    <FormDescription>
                      Developers can ask to promote; a maintainer approves
                      before anything changes in the store.
                    </FormDescription>
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

            {appStoreVersion ? (
              <>
                <FormField
                  control={form.control}
                  name="phased_release"
                  render={({ field }) => (
                    <FormItem className="flex flex-row items-start justify-between gap-4 rounded-lg border p-3">
                      <Stack gap={1}>
                        <FormLabel>Phased release</FormLabel>
                        <FormDescription>
                          Release over 7 days on Apple&apos;s schedule. Off
                          releases to everyone once Apple approves it.
                        </FormDescription>
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
                        <FormLabel>Submit for review</FormLabel>
                        <FormDescription>
                          Submit the version for App Review once a promotion
                          attaches the build. Off leaves it for you to submit in
                          App Store Connect.
                        </FormDescription>
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

            {staged ? (
              <>
                <FormField
                  control={form.control}
                  name="steps"
                  render={({ field }) => (
                    <FormItem>
                      <FormLabel>Rollout steps (%)</FormLabel>
                      <FormControl>
                        <Input
                          autoComplete="off"
                          className="font-mono"
                          placeholder="1, 5, 20, 50, 100"
                          {...field}
                        />
                      </FormControl>
                      <FormDescription>
                        A promoted release starts at the first step and ends at
                        100.
                      </FormDescription>
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
                {isEdit ? 'Save changes' : 'Add environment'}
              </Button>
            </DialogFooter>
          </form>
        </Form>
      </DialogContent>
    </Dialog>
  )
}
