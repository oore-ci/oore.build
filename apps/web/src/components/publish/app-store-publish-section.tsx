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
  useUpdatePipelineAppStorePublish,
  type AppStorePublishConfig,
  type AppStorePublishResponse,
} from '@/hooks/use-pipeline-publish'
import { CollectionError } from '@/components/collection'
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
  })
  .superRefine((values, ctx) => {
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
  }
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
  }
}

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
          description={effect}
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
  name: 'uses_non_exempt_encryption' | 'submit_for_review' | 'phased_release'
}) {
  return (
    <FormField
      control={form.control}
      name={name}
      render={({ field }) => (
        <FormItem className="flex flex-row items-start gap-3">
          <FormControl>
            <Switch
              checked={field.value}
              onCheckedChange={field.onChange}
              disabled={disabled}
              aria-label={label}
            />
          </FormControl>
          <div className="flex flex-col gap-1">
            <FormLabel>{label}</FormLabel>
            <FormDescription>{description}</FormDescription>
          </div>
        </FormItem>
      )}
    />
  )
}

function DestinationFields({ form }: { form: UseFormReturn<FormValues> }) {
  const [destination, submitForReview] = useWatch({
    control: form.control,
    name: ['destination', 'submit_for_review'],
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
        <div className="flex flex-col gap-4">
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
        </div>
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
    <div className="flex flex-col gap-4">
      <p className="text-sm text-muted-foreground">
        {destination === 'testflight'
          ? 'Testers see this in TestFlight. Locales without text are left as they are in App Store Connect.'
          : 'Users see this on the App Store. Locales without text are left as they are in App Store Connect.'}
      </p>
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
    </div>
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
  const isNew = saved.updated_at == null

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
      <form onSubmit={(event) => void form.handleSubmit(onSubmit)(event)}>
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
                    disabled={updateMutation.isPending || (!isNew && !isDirty)}
                  >
                    {updateMutation.isPending ? (
                      <Spinner data-icon="inline-start" />
                    ) : null}
                    {isNew ? 'Save App Store settings' : 'Save changes'}
                  </Button>
                </div>
              </>
            ) : null}
          </SettingsSurface>
        </fieldset>
      </form>
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
  const missing = data
    ? [
        data.app_store_export
          ? null
          : 'set Distribution to App Store in iOS signing',
        data.has_api_key ? null : 'add an App Store Connect API key',
      ].filter((item) => item !== null)
    : []

  return (
    <SettingsSection
      title="App Store Connect"
      description="Upload the signed IPA to TestFlight or the App Store. The upload runs on the Mac that built the app, using the API key from this pipeline’s iOS signing."
    >
      {missing.length > 0 ? (
        <Alert>
          <HugeiconsIcon icon={InformationCircleIcon} />
          <AlertTitle>
            iOS signing is not ready for App Store uploads
          </AlertTitle>
          <AlertDescription className="flex flex-col items-start gap-2">
            <span>
              Before turning this on, {missing.join(' and ')}. Until then,
              saving with uploads on is refused.
            </span>
            {canWrite ? (
              <Button
                variant="outline"
                size="sm"
                nativeButton={false}
                render={
                  <Link
                    to="/projects/$projectId/pipelines/$pipelineId/edit"
                    params={{ projectId, pipelineId }}
                    search={{}}
                  />
                }
              >
                Edit iOS signing
              </Button>
            ) : null}
          </AlertDescription>
        </Alert>
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
