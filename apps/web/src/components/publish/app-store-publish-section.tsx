import { Link } from '@tanstack/react-router'
import {
  useFieldArray,
  useForm,
  useFormState,
  useWatch,
  type UseFormReturn,
} from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import * as z from 'zod'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  Add01Icon,
  Delete02Icon,
  InformationCircleIcon,
} from '@hugeicons/core-free-icons'

import { getApiErrorMessage } from '@/lib/api-client/api-error'
import { toast } from '@/lib/toast'
import {
  usePipelineAppStorePublish,
  usePipelinePublishReadiness,
  useUpdatePipelineAppStorePublish,
  type AppStorePublishConfig,
  type AppStorePublishResponse,
} from '@/hooks/use-pipeline-publish'
import { CollectionError } from '@/components/collection'
import { UnsavedChangesGuard } from '@/components/unsaved-changes-guard'
import {
  PublishReadiness,
  type ReadinessCheck,
} from '@/components/publish/publish-readiness'
import {
  SettingsSection,
  SettingsSurface,
  SettingsSurfaceSeparator,
} from '@/components/settings/settings-section'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
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
import { RadioGroup, RadioGroupItem } from '@/components/ui/radio-group'
import { Skeleton } from '@/components/ui/skeleton'
import { Spinner } from '@/components/ui/spinner'
import { Switch } from '@/components/ui/switch'
import { Textarea } from '@/components/ui/textarea'
import { Stack } from '@/components/layout/stack'
import { Text } from '@/components/layout/text'

// Limits match the daemon's validation in app_store_publish.rs.
const MAX_GROUPS = 20
const MAX_GROUP_NAME_LENGTH = 128
const MAX_NOTES = 40
const MAX_NOTE_LENGTH = 4000
const LOCALE_PATTERN = /^[A-Za-z0-9]+(-[A-Za-z0-9]+)*$/

const DESTINATIONS = {
  testflight: 'TestFlight',
  app_store: 'App Store',
} as const

const formSchema = z
  .object({
    enabled: z.boolean(),
    destination: z.enum(['testflight', 'app_store']),
    testflight_groups: z.string(),
    release_notes: z.array(
      z.object({
        locale: z.string().trim(),
        whats_new: z
          .string()
          .trim()
          .max(
            MAX_NOTE_LENGTH,
            `App Store Connect allows at most ${MAX_NOTE_LENGTH} characters.`,
          ),
      }),
    ),
    uses_non_exempt_encryption: z.boolean(),
    submit_for_review: z.boolean(),
    phased_release: z.boolean(),
    release_type: z.enum(['after_approval', 'manual', 'scheduled']),
    earliest_release_date: z.string(),
    cancel_previous_submission: z.boolean(),
    release_notes_dir: z.string().trim(),
    listing_dir: z.string().trim(),
    preflight: z.enum(['auto', 'block', 'warn', 'off']),
  })
  .superRefine((values, ctx) => {
    if (
      values.destination === 'app_store' &&
      values.release_type === 'scheduled' &&
      !values.earliest_release_date
    ) {
      ctx.addIssue({
        code: 'custom',
        path: ['earliest_release_date'],
        message: 'Choose when Apple releases the version.',
      })
    }
    if (
      values.release_notes_dir.startsWith('/') ||
      values.release_notes_dir.split('/').includes('..')
    ) {
      ctx.addIssue({
        code: 'custom',
        path: ['release_notes_dir'],
        message: 'Use a path inside the repository, such as fastlane/metadata.',
      })
    }
    if (
      values.listing_dir.startsWith('/') ||
      values.listing_dir.split('/').includes('..')
    ) {
      ctx.addIssue({
        code: 'custom',
        path: ['listing_dir'],
        message: 'Use a path inside the repository, such as fastlane/metadata.',
      })
    }
    const groups = parseGroups(values.testflight_groups)
    if (groups.some((group) => group.length > MAX_GROUP_NAME_LENGTH)) {
      ctx.addIssue({
        code: 'custom',
        path: ['testflight_groups'],
        message: `Group names can be at most ${MAX_GROUP_NAME_LENGTH} characters.`,
      })
    } else if (groups.length > MAX_GROUPS) {
      ctx.addIssue({
        code: 'custom',
        path: ['testflight_groups'],
        message: `List at most ${MAX_GROUPS} groups.`,
      })
    } else if (
      values.enabled &&
      values.destination === 'testflight' &&
      groups.length === 0
    ) {
      ctx.addIssue({
        code: 'custom',
        path: ['testflight_groups'],
        message: 'Name at least one TestFlight group.',
      })
    }
    const seen = new Set<string>()
    values.release_notes.forEach((note, index) => {
      if (!note.locale && !note.whats_new) return
      if (!LOCALE_PATTERN.test(note.locale) || note.locale.length > 16) {
        ctx.addIssue({
          code: 'custom',
          path: ['release_notes', index, 'locale'],
          message: 'Enter an App Store Connect locale such as en-US.',
        })
      } else if (seen.has(note.locale)) {
        ctx.addIssue({
          code: 'custom',
          path: ['release_notes', index, 'locale'],
          message: 'This locale is already listed.',
        })
      }
      seen.add(note.locale)
    })
  })

type FormValues = z.infer<typeof formSchema>

function parseGroups(raw: string) {
  return [
    ...new Set(
      raw
        .split('\n')
        .map((group) => group.trim())
        .filter(Boolean),
    ),
  ]
}

function toFormValues(config: AppStorePublishConfig): FormValues {
  return {
    enabled: config.enabled,
    destination: config.destination ?? 'testflight',
    testflight_groups: config.testflight_groups.join('\n'),
    release_notes:
      config.release_notes.length > 0
        ? config.release_notes
        : [{ locale: 'en-US', whats_new: '' }],
    uses_non_exempt_encryption: config.uses_non_exempt_encryption ?? false,
    submit_for_review: config.submit_for_review ?? false,
    phased_release: config.phased_release ?? false,
    release_type: config.release_type ?? 'after_approval',
    earliest_release_date: toLocalDateTime(config.earliest_release_date),
    cancel_previous_submission: config.cancel_previous_submission ?? false,
    release_notes_dir: config.release_notes_dir ?? '',
    listing_dir: config.listing_dir ?? '',
    preflight: config.preflight ?? 'auto',
  }
}

/** An RFC 3339 time as a `datetime-local` value in the viewer's zone. */
function toLocalDateTime(value: string | null | undefined): string {
  if (!value) return ''
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return ''
  const local = new Date(date.getTime() - date.getTimezoneOffset() * 60_000)
  return local.toISOString().slice(0, 16)
}

/** A `datetime-local` value as the RFC 3339 time App Store Connect takes. */
function fromLocalDateTime(value: string): string | null {
  if (!value) return null
  const date = new Date(value)
  return Number.isNaN(date.getTime())
    ? null
    : date.toISOString().replace(/\.\d{3}Z$/, 'Z')
}

function toConfig(values: FormValues): AppStorePublishConfig {
  const appStore = values.destination === 'app_store'
  return {
    enabled: values.enabled,
    destination: values.destination,
    testflight_groups: parseGroups(values.testflight_groups),
    release_notes: values.release_notes.filter(
      (note) => note.locale && note.whats_new,
    ),
    uses_non_exempt_encryption: values.uses_non_exempt_encryption,
    submit_for_review: appStore && values.submit_for_review,
    phased_release: appStore && values.phased_release,
    release_type: appStore ? values.release_type : 'after_approval',
    earliest_release_date:
      appStore && values.release_type === 'scheduled'
        ? fromLocalDateTime(values.earliest_release_date)
        : null,
    cancel_previous_submission: appStore && values.cancel_previous_submission,
    release_notes_dir: values.release_notes_dir || null,
    listing_dir: values.listing_dir || null,
    preflight: values.preflight,
  }
}

const RELEASE_TYPES = {
  after_approval: 'When approved',
  manual: 'When I release it',
  scheduled: 'On a date',
} as const

const PREFLIGHT_LEVELS = {
  auto: 'Automatic',
  block: 'Stop the upload',
  warn: 'Warn only',
  off: 'Off',
} as const

const EFFECT_FIELDS = [
  'enabled',
  'destination',
  'testflight_groups',
  'submit_for_review',
  'phased_release',
] as const

type EffectValues = Pick<FormValues, (typeof EFFECT_FIELDS)[number]>

/** One sentence describing what the saved settings will do to a build. */
function describeEffect(values: EffectValues) {
  if (!values.enabled) {
    return 'Builds of this pipeline are not uploaded to App Store Connect.'
  }
  if (values.destination === 'testflight') {
    const groups = parseGroups(values.testflight_groups)
    const target =
      groups.length > 0 ? groups.join(', ') : 'the TestFlight groups below'
    return `After each successful build, the signed IPA is uploaded to App Store Connect and, once Apple finishes processing it, added to ${target} in TestFlight.`
  }
  const review = values.submit_for_review
    ? ' and submitted for review'
    : ', ready for you to submit from App Store Connect'
  const release =
    values.submit_for_review && values.phased_release
      ? ' Once approved, Apple releases it to users over seven days.'
      : ''
  return `After each successful build, the signed IPA is uploaded to App Store Connect and attached to the App Store version matching its version string${review}.${release}`
}

function UploadToggle({ form }: { form: UseFormReturn<FormValues> }) {
  const [
    enabled,
    destination,
    testflight_groups,
    submit_for_review,
    phased_release,
  ] = useWatch({ control: form.control, name: EFFECT_FIELDS })
  const effect = describeEffect({
    enabled,
    destination,
    testflight_groups,
    submit_for_review,
    phased_release,
  })

  return (
    <FormField
      control={form.control}
      name="enabled"
      render={({ field }) => (
        <SettingsSection
          title="Upload after each build"
          status={effect}
          actions={
            <FormItem>
              <FormControl>
                <Switch
                  checked={field.value}
                  onCheckedChange={field.onChange}
                  aria-label="Upload to App Store Connect after each successful build"
                />
              </FormControl>
            </FormItem>
          }
        />
      )}
    />
  )
}

function SwitchField({
  description,
  disabled,
  form,
  label,
  name,
}: {
  description: string
  disabled?: boolean
  form: UseFormReturn<FormValues>
  label: string
  name:
    | 'uses_non_exempt_encryption'
    | 'submit_for_review'
    | 'phased_release'
    | 'cancel_previous_submission'
}) {
  return (
    <FormField
      control={form.control}
      name={name}
      render={({ field }) => (
        <FormItem className="flex flex-row items-start justify-between gap-4">
          <Stack gap={1}>
            <FormLabel>{label}</FormLabel>
            <FormDescription>{description}</FormDescription>
          </Stack>
          <FormControl>
            <Switch
              checked={field.value}
              onCheckedChange={field.onChange}
              disabled={disabled}
              aria-label={label}
            />
          </FormControl>
        </FormItem>
      )}
    />
  )
}

function DestinationFields({ form }: { form: UseFormReturn<FormValues> }) {
  const [destination, submitForReview, releaseType] = useWatch({
    control: form.control,
    name: ['destination', 'submit_for_review', 'release_type'],
  })

  return (
    <>
      <FormField
        control={form.control}
        name="destination"
        render={({ field }) => (
          <FormItem>
            <FormLabel className="sr-only">Destination</FormLabel>
            <FormControl>
              <RadioGroup
                value={field.value}
                onValueChange={field.onChange}
                className="flex flex-wrap gap-x-6 gap-y-2"
              >
                {Object.entries(DESTINATIONS).map(([value, label]) => (
                  <label
                    key={value}
                    className="flex items-center gap-2 text-sm"
                  >
                    <RadioGroupItem value={value} />
                    {label}
                  </label>
                ))}
              </RadioGroup>
            </FormControl>
            <FormDescription>
              {field.value === 'testflight'
                ? 'Internal groups get the build as soon as it is processed. External groups wait for TestFlight beta review first.'
                : 'Apple creates the version if it does not exist yet. Nothing reaches users until the version is approved and released.'}
            </FormDescription>
            <FormMessage />
          </FormItem>
        )}
      />

      {destination === 'testflight' ? (
        <FormField
          control={form.control}
          name="testflight_groups"
          render={({ field }) => (
            <FormItem>
              <FormLabel>TestFlight groups</FormLabel>
              <FormControl>
                <Textarea
                  rows={3}
                  placeholder={'Internal QA\nBeta testers'}
                  spellCheck={false}
                  {...field}
                />
              </FormControl>
              <FormDescription>
                One group name per line, as it appears in App Store Connect. A
                name that doesn’t exist fails the release.
              </FormDescription>
              <FormMessage />
            </FormItem>
          )}
        />
      ) : (
        <Stack gap={4}>
          <SwitchField
            form={form}
            name="submit_for_review"
            label="Submit for review"
            description="Send the version to App Review as soon as the build is attached. Off leaves the submission to you."
          />
          <SwitchField
            form={form}
            name="phased_release"
            label="Phased release"
            disabled={!submitForReview}
            description={
              submitForReview
                ? 'After approval, Apple releases to 1, 2, 5, 10, 20, 50 and then 100% of automatic updates over seven days. Pause or complete it from the release board.'
                : 'Available when Oore submits the version for review.'
            }
          />
          <SwitchField
            form={form}
            name="cancel_previous_submission"
            label="Replace an open submission"
            disabled={!submitForReview}
            description="Apple allows one open review submission per app. On, Oore cancels one that holds other items before submitting; off, the release is blocked until you cancel it."
          />
          <FormField
            control={form.control}
            name="release_type"
            render={({ field }) => (
              <FormItem>
                <FormLabel>Release after approval</FormLabel>
                <FormControl>
                  <RadioGroup
                    value={field.value}
                    onValueChange={field.onChange}
                    className="flex flex-wrap gap-x-6 gap-y-2"
                  >
                    {Object.entries(RELEASE_TYPES).map(([value, label]) => (
                      <label
                        key={value}
                        className="flex items-center gap-2 text-sm"
                      >
                        <RadioGroupItem value={value} />
                        {label}
                      </label>
                    ))}
                  </RadioGroup>
                </FormControl>
                <FormDescription>
                  {field.value === 'manual'
                    ? 'Approved versions wait for you. Release them from the release board with Release now.'
                    : field.value === 'scheduled'
                      ? 'Apple releases the approved version at this time, or on approval if the time has passed.'
                      : 'Apple puts the version on the App Store as soon as App Review approves it.'}
                </FormDescription>
                <FormMessage />
              </FormItem>
            )}
          />
          {releaseType === 'scheduled' ? (
            <FormField
              control={form.control}
              name="earliest_release_date"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>Release on</FormLabel>
                  <FormControl>
                    <Input type="datetime-local" className="w-fit" {...field} />
                  </FormControl>
                  <FormMessage />
                </FormItem>
              )}
            />
          ) : null}
        </Stack>
      )}
    </>
  )
}

function ReleaseNotesFields({ form }: { form: UseFormReturn<FormValues> }) {
  const destination = useWatch({ control: form.control, name: 'destination' })
  const { fields, append, remove } = useFieldArray({
    control: form.control,
    name: 'release_notes',
  })
  const label = destination === 'testflight' ? 'What to Test' : 'What’s New'

  return (
    <Stack gap={4}>
      <Text variant="muted">
        {destination === 'testflight'
          ? 'Testers see this in TestFlight. Locales without text are left as they are in App Store Connect.'
          : 'Users see this on the App Store. Locales without text are left as they are in App Store Connect.'}
      </Text>
      {fields.map((item, index) => (
        <div
          key={item.id}
          className="grid gap-3 sm:grid-cols-[8rem_1fr_auto] sm:items-start"
        >
          <FormField
            control={form.control}
            name={`release_notes.${index}.locale`}
            render={({ field }) => (
              <FormItem>
                <FormLabel className="sm:sr-only">Locale</FormLabel>
                <FormControl>
                  <Input
                    className="font-mono"
                    placeholder="en-US"
                    aria-label={`Locale ${index + 1}`}
                    {...field}
                  />
                </FormControl>
                <FormMessage />
              </FormItem>
            )}
          />
          <FormField
            control={form.control}
            name={`release_notes.${index}.whats_new`}
            render={({ field }) => (
              <FormItem>
                <FormLabel className="sm:sr-only">{label}</FormLabel>
                <FormControl>
                  <Textarea
                    rows={3}
                    aria-label={`${label} ${index + 1}`}
                    {...field}
                  />
                </FormControl>
                <FormDescription>
                  {field.value.length} / {MAX_NOTE_LENGTH}
                </FormDescription>
                <FormMessage />
              </FormItem>
            )}
          />
          <Button
            type="button"
            variant="ghost"
            size="icon-sm"
            onClick={() => remove(index)}
            aria-label={`Remove locale ${index + 1}`}
          >
            <HugeiconsIcon icon={Delete02Icon} />
          </Button>
        </div>
      ))}
      <FormField
        control={form.control}
        name="release_notes_dir"
        render={({ field }) => (
          <FormItem>
            <FormLabel>Notes from the repository</FormLabel>
            <FormControl>
              <Input
                className="font-mono"
                placeholder="fastlane/metadata"
                spellCheck={false}
                {...field}
              />
            </FormControl>
            <FormDescription>
              Optional. A directory with one file per locale, such as en-US.txt,
              or fastlane’s en-US/release_notes.txt. When a build has it, its
              notes replace the ones above.
            </FormDescription>
            <FormMessage />
          </FormItem>
        )}
      />
      <FormField
        control={form.control}
        name="listing_dir"
        render={({ field }) => (
          <FormItem>
            <FormLabel>Store listing from the repository</FormLabel>
            <FormControl>
              <Input
                className="font-mono"
                placeholder="fastlane/metadata"
                spellCheck={false}
                {...field}
              />
            </FormControl>
            <FormDescription>
              Optional. fastlane’s layout: en-US/description.txt, keywords.txt,
              promotional_text.txt, marketing_url.txt and support_url.txt. Each
              version Oore submits gets them; files you leave out stay as they
              are in App Store Connect. Screenshots are not uploaded.
            </FormDescription>
            <FormMessage />
          </FormItem>
        )}
      />
      {fields.length < MAX_NOTES ? (
        <div>
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={() => append({ locale: '', whats_new: '' })}
          >
            <HugeiconsIcon icon={Add01Icon} />
            Add locale
          </Button>
        </div>
      ) : null}
    </Stack>
  )
}

function AppStorePublishForm({
  canWrite,
  pipelineId,
  saved,
}: {
  canWrite: boolean
  pipelineId: string
  saved: AppStorePublishResponse
}) {
  const updateMutation = useUpdatePipelineAppStorePublish(pipelineId)
  const form = useForm<FormValues>({
    resolver: zodResolver(formSchema),
    values: toFormValues(saved.config),
    resetOptions: { keepDirtyValues: true },
  })
  const { isDirty } = useFormState({ control: form.control })
  const enabled = useWatch({ control: form.control, name: 'enabled' })
  const isNew = saved.updated_at == null
  // Same rule as Google Play: Save needs a change, except that a first
  // setup with uploads on can be saved as it stands.
  const canSave = isDirty || (isNew && enabled)

  function onSubmit(next: FormValues) {
    updateMutation.mutate(toConfig(next), {
      onSuccess: (response) => {
        form.reset(toFormValues(response.config))
        toast.success('App Store settings saved')
      },
    })
  }

  return (
    <Form {...form}>
      <form
        noValidate
        onSubmit={(event) => void form.handleSubmit(onSubmit)(event)}
      >
        <fieldset disabled={!canWrite} className="min-w-0">
          <SettingsSurface className="flex flex-col gap-6">
            <UploadToggle form={form} />

            <SettingsSurfaceSeparator />

            <SettingsSection title="Destination">
              <DestinationFields form={form} />
            </SettingsSection>

            <SettingsSurfaceSeparator />

            <SettingsSection title="Release notes">
              <ReleaseNotesFields form={form} />
            </SettingsSection>

            <SettingsSurfaceSeparator />

            <SettingsSection title="Pre-flight checks">
              <FormField
                control={form.control}
                name="preflight"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel className="sr-only">Pre-flight checks</FormLabel>
                    <FormControl>
                      <RadioGroup
                        value={field.value}
                        onValueChange={field.onChange}
                        className="flex flex-wrap gap-x-6 gap-y-2"
                      >
                        {Object.entries(PREFLIGHT_LEVELS).map(
                          ([value, label]) => (
                            <label
                              key={value}
                              className="flex items-center gap-2 text-sm"
                            >
                              <RadioGroupItem value={value} />
                              {label}
                            </label>
                          ),
                        )}
                      </RadioGroup>
                    </FormControl>
                    <FormDescription>
                      Before uploading, the runner checks for what Apple refuses
                      after the upload, such as an app icon with transparency.
                      Automatic warns until a release from this pipeline has
                      been accepted, then stops the upload.
                    </FormDescription>
                  </FormItem>
                )}
              />
            </SettingsSection>

            <SettingsSurfaceSeparator />

            <SettingsSection title="Export compliance">
              <SwitchField
                form={form}
                name="uses_non_exempt_encryption"
                label="Uses non-exempt encryption"
                description="Oore answers Apple’s export compliance question with this on every build. Leave it off if the app only uses encryption built into iOS, such as HTTPS."
              />
            </SettingsSection>

            {updateMutation.error ? (
              <Alert variant="destructive">
                <HugeiconsIcon icon={InformationCircleIcon} />
                <AlertTitle>Settings were not saved</AlertTitle>
                <AlertDescription>
                  {getApiErrorMessage(updateMutation.error, {})}
                </AlertDescription>
              </Alert>
            ) : null}

            {canWrite ? (
              <>
                <SettingsSurfaceSeparator />
                <div className="flex justify-end gap-2">
                  {isDirty && !isNew ? (
                    <Button
                      type="button"
                      variant="outline"
                      onClick={() => form.reset(toFormValues(saved.config))}
                    >
                      Discard changes
                    </Button>
                  ) : null}
                  <Button
                    type="submit"
                    disabled={updateMutation.isPending || !canSave}
                  >
                    {updateMutation.isPending ? (
                      <Spinner data-icon="inline-start" />
                    ) : null}
                    {isNew && enabled
                      ? 'Turn on App Store publishing'
                      : 'Save changes'}
                  </Button>
                </div>
              </>
            ) : null}
          </SettingsSurface>
        </fieldset>
      </form>
      <UnsavedChangesGuard
        when={isDirty && !updateMutation.isPending}
        title="Discard App Store changes?"
        description="Your changes to the App Store Connect settings have not been saved."
      />
    </Form>
  )
}

export function AppStorePublishSection({
  canWrite,
  pipelineId,
  projectId,
}: {
  canWrite: boolean
  pipelineId: string
  projectId: string
}) {
  const publishQuery = usePipelineAppStorePublish(pipelineId)
  const data = publishQuery.data
  const storeReadiness = usePipelinePublishReadiness(pipelineId, {
    enabled: !!data?.config.enabled,
  })
  const editLink = (label: string) =>
    canWrite ? (
      <Button
        variant="outline"
        size="sm"
        nativeButton={false}
        render={
          <Link
            to="/projects/$projectId/pipelines/$pipelineId/edit"
            params={{ projectId, pipelineId }}
            search={{}}
            hash="ios-signing"
          />
        }
      >
        {label}
      </Button>
    ) : undefined
  const checks: Array<ReadinessCheck> = data
    ? [
        {
          key: 'distribution',
          label: 'App Store distribution',
          ready: data.app_store_export,
          detail: data.app_store_export
            ? 'iOS signing uses an App Store distribution profile.'
            : 'iOS signing must use App Store distribution. Ad hoc builds cannot go to TestFlight or the App Store.',
          action: data.app_store_export
            ? undefined
            : editLink('Edit iOS signing'),
        },
        {
          key: 'key',
          label: 'App Store Connect API key',
          ready: data.has_api_key,
          detail: data.has_api_key
            ? 'Uploads use the API key from this pipeline’s iOS signing.'
            : 'Choose a shared key or add this pipeline’s own key in iOS signing.',
          action: data.has_api_key ? undefined : editLink('Add API key'),
        },
        ...(storeReadiness.data?.items ?? [])
          .filter(
            (item) => item.store === 'app_store' && item.code !== 'api_key',
          )
          .map((item): ReadinessCheck => ({
            key: `store-${item.code}`,
            label: item.remembered
              ? `${item.title} (Apple’s last refusal)`
              : item.title,
            ready: item.level === 'ok',
            detail: item.detail ?? '',
            action: item.console_url ? (
              <Button
                variant="outline"
                size="sm"
                nativeButton={false}
                render={
                  <a href={item.console_url} target="_blank" rel="noreferrer" />
                }
              >
                Open in App Store Connect
              </Button>
            ) : undefined,
          })),
      ]
    : []
  // Only the setup checks stop a save; store findings stop a submission.
  const blocked = checks
    .filter((check) => !check.key.startsWith('store-'))
    .some((check) => !check.ready)

  return (
    <SettingsSection
      id="app-store"
      title="App Store Connect"
      hint="The Mac that built the app uploads the signed IPA with the API key from iOS signing."
    >
      {checks.length > 0 ? (
        <PublishReadiness
          checks={checks}
          footnote={
            blocked
              ? 'Saving with uploads on is refused until the steps above are done.'
              : undefined
          }
        />
      ) : null}

      {publishQuery.error ? (
        <CollectionError
          title="Failed to load App Store settings"
          description={publishQuery.error.message}
          onRetry={() => void publishQuery.refetch()}
        />
      ) : data ? (
        <AppStorePublishForm
          canWrite={canWrite}
          pipelineId={pipelineId}
          saved={data}
        />
      ) : (
        <Skeleton className="h-96 w-full" />
      )}
    </SettingsSection>
  )
}
