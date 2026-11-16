import { Link } from '@tanstack/react-router'
import {
  useForm,
  useFormState,
  useWatch,
  type UseFormReturn,
} from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import * as z from 'zod'
import { HugeiconsIcon } from '@hugeicons/react'
import { InformationCircleIcon } from '@hugeicons/core-free-icons'

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
import { AdvancedOptions } from '@/components/publish/advanced-options'
import {
  FileControlledLine,
  NotConnectedLine,
  PartlyFileControlledNote,
  useFilePublishing,
  type FilePublishing,
} from '@/components/publish/publish-line'
import { useProjectStores } from '@/hooks/use-project-stores'
import { InfoHint } from '@/components/ui/info-hint'
import {
  PublishReadiness,
  type ReadinessCheck,
} from '@/components/publish/publish-readiness'
import {
  SettingsSection,
  SettingsSurface,
} from '@/components/settings/settings-section'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import {
  Form,
  FormControl,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
} from '@/components/ui/form'
import { Input } from '@/components/ui/input'
import { RadioGroup, RadioGroupItem } from '@/components/ui/radio-group'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Skeleton } from '@/components/ui/skeleton'
import { Spinner } from '@/components/ui/spinner'
import { Switch } from '@/components/ui/switch'
import { Textarea } from '@/components/ui/textarea'
import { Stack } from '@/components/layout/stack'

// Limits match the daemon's validation in app_store_publish.rs.
const MAX_GROUPS = 20
const MAX_GROUP_NAME_LENGTH = 128

const formSchema = z
  .object({
    enabled: z.boolean(),
    destination: z.enum(['testflight', 'app_store']),
    testflight_groups: z.string(),
    uses_non_exempt_encryption: z.boolean(),
    submit_for_review: z.boolean(),
    phased_release: z.boolean(),
    release_type: z.enum(['after_approval', 'manual', 'scheduled']),
    earliest_release_date: z.string(),
    cancel_previous_submission: z.boolean(),
    release_notes_dir: z.string().trim(),
    listing_dir: z.string().trim(),
    preflight: z.enum(['auto', 'block', 'warn', 'off']),
    upload_with_signing_key: z.boolean(),
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
    uses_non_exempt_encryption: config.uses_non_exempt_encryption ?? false,
    submit_for_review: config.submit_for_review ?? false,
    phased_release: config.phased_release ?? false,
    release_type: config.release_type ?? 'after_approval',
    earliest_release_date: toLocalDateTime(config.earliest_release_date),
    cancel_previous_submission: config.cancel_previous_submission ?? false,
    release_notes_dir: config.release_notes_dir ?? '',
    listing_dir: config.listing_dir ?? '',
    preflight: config.preflight ?? 'auto',
    upload_with_signing_key: config.upload_with_signing_key ?? false,
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

/**
 * The form's values as a config. Release notes now belong to each app
 * version, so notes saved here by older versions of Oore are kept as they
 * are: the daemon only reads them to seed a project's first version.
 */
function toConfig(
  values: FormValues,
  saved: AppStorePublishConfig,
): AppStorePublishConfig {
  const appStore = values.destination === 'app_store'
  return {
    enabled: values.enabled,
    destination: values.destination,
    testflight_groups: parseGroups(values.testflight_groups),
    release_notes: saved.release_notes,
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
    upload_with_signing_key: values.upload_with_signing_key,
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

const LINE_ITEMS = {
  off: 'Nowhere',
  testflight: 'TestFlight',
  app_store: 'App Store',
} as const

/** "After a successful build, send to" and the destination, one line. */
function DestinationLine({ form }: { form: UseFormReturn<FormValues> }) {
  const [enabled, destination] = useWatch({
    control: form.control,
    name: ['enabled', 'destination'],
  })
  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
        <label htmlFor="app-store-destination" className="text-sm">
          After a successful build, send to
        </label>
        <Select
          value={enabled ? destination : 'off'}
          onValueChange={(next) => {
            if (next === 'testflight' || next === 'app_store') {
              form.setValue('enabled', true, { shouldDirty: true })
              form.setValue('destination', next, { shouldDirty: true })
            } else {
              form.setValue('enabled', false, { shouldDirty: true })
            }
          }}
          items={LINE_ITEMS}
        >
          <SelectTrigger id="app-store-destination" className="min-w-48">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {Object.entries(LINE_ITEMS).map(([key, label]) => (
              <SelectItem key={key} value={key}>
                {label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        {enabled ? (
          <InfoHint label="Destination">
            {destination === 'testflight'
              ? 'Internal groups get the build as soon as it is processed. External groups wait for TestFlight beta review first.'
              : 'Apple creates the version if it does not exist yet. Nothing reaches users until the version is approved and released.'}
          </InfoHint>
        ) : null}
      </div>
      {enabled && destination === 'testflight' ? (
        <FormField
          control={form.control}
          name="testflight_groups"
          render={({ field }) => (
            <FormItem className="max-w-md">
              <FormLabel
                hint={
                  'One group name per line, as it appears in App Store Connect. A name that doesn’t exist fails the release.'
                }
              >
                TestFlight groups
              </FormLabel>
              <FormControl>
                <Textarea
                  rows={2}
                  placeholder={'Internal QA\nBeta testers'}
                  spellCheck={false}
                  {...field}
                />
              </FormControl>
              <FormMessage />
            </FormItem>
          )}
        />
      ) : null}
    </div>
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
    | 'upload_with_signing_key'
}) {
  return (
    <FormField
      control={form.control}
      name={name}
      render={({ field }) => (
        <FormItem className="flex flex-row items-start justify-between gap-4">
          <Stack gap={1}>
            <FormLabel hint={description}>{label}</FormLabel>
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

function AppStoreReleaseFields({ form }: { form: UseFormReturn<FormValues> }) {
  const [submitForReview, releaseType] = useWatch({
    control: form.control,
    name: ['submit_for_review', 'release_type'],
  })

  return (
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
            <FormLabel
              hint={
                field.value === 'manual'
                  ? 'Approved versions wait for you. Release them from the release board with Release now.'
                  : field.value === 'scheduled'
                    ? 'Apple releases the approved version at this time, or on approval if the time has passed.'
                    : 'Apple puts the version on the App Store as soon as App Review approves it.'
              }
            >
              Release after approval
            </FormLabel>
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
  )
}

const ADVANCED_FIELDS = [
  'release_notes_dir',
  'listing_dir',
  'preflight',
  'uses_non_exempt_encryption',
  'submit_for_review',
  'upload_with_signing_key',
  'destination',
] as const

function AdvancedFields({
  appKeyName,
  form,
}: {
  /** The app's App Store Connect key, when the app has one. */
  appKeyName: string | undefined
  form: UseFormReturn<FormValues>
}) {
  const [notesDir, listingDir, , encryption, submit, ownKey, destination] =
    useWatch({
      control: form.control,
      name: ADVANCED_FIELDS,
    })
  const { errors } = useFormState({ control: form.control })
  const summary = [
    destination === 'app_store'
      ? submit
        ? 'Submits for review'
        : 'No review submission'
      : null,
    notesDir ? 'notes from files' : null,
    listingDir ? 'store listing' : null,
    encryption ? 'non-exempt encryption' : null,
    ownKey ? 'signing key' : null,
  ]
    .filter(Boolean)
    .join(' · ')
  return (
    <AdvancedOptions
      changed={false}
      hasError={ADVANCED_FIELDS.some((name) => !!errors[name])}
      summary={summary || 'Notes, store listing, pre-flight checks'}
    >
      {destination === 'app_store' ? (
        <AppStoreReleaseFields form={form} />
      ) : null}
      {appKeyName ? (
        <SwitchField
          form={form}
          name="upload_with_signing_key"
          label="Upload with this pipeline’s signing key"
          description={`Off uses the app’s key, ${appKeyName}.`}
        />
      ) : null}
      <FormField
        control={form.control}
        name="release_notes_dir"
        render={({ field }) => (
          <FormItem>
            <FormLabel
              hint={
                'Release notes belong to each app version: Oore drafts them from commits and you edit them on the build page. To keep them in the repository instead, name a directory with one file per locale, such as en-US.txt, or fastlane’s en-US/release_notes.txt. A release_notes entry in .oore.yaml does the same for every store.'
              }
            >
              Release notes from the repository
            </FormLabel>
            <FormControl>
              <Input
                className="font-mono"
                placeholder="fastlane/metadata"
                spellCheck={false}
                {...field}
              />
            </FormControl>
            <FormMessage />
          </FormItem>
        )}
      />
      <FormField
        control={form.control}
        name="listing_dir"
        render={({ field }) => (
          <FormItem>
            <FormLabel
              hint={
                'fastlane’s layout: en-US/description.txt, keywords.txt, promotional_text.txt, marketing_url.txt and support_url.txt. Each version Oore submits gets them; files you leave out stay as they are in App Store Connect. Screenshots are not uploaded.'
              }
            >
              Store listing from the repository
            </FormLabel>
            <FormControl>
              <Input
                className="font-mono"
                placeholder="fastlane/metadata"
                spellCheck={false}
                {...field}
              />
            </FormControl>
            <FormMessage />
          </FormItem>
        )}
      />
      <FormField
        control={form.control}
        name="preflight"
        render={({ field }) => (
          <FormItem>
            <FormLabel
              hint={
                'Before uploading, the runner checks for what Apple refuses after the upload, such as an app icon with transparency. Automatic warns until a release from this pipeline has been accepted, then stops the upload.'
              }
            >
              Pre-flight checks
            </FormLabel>
            <FormControl>
              <RadioGroup
                value={field.value}
                onValueChange={field.onChange}
                className="flex flex-wrap gap-x-6 gap-y-2"
              >
                {Object.entries(PREFLIGHT_LEVELS).map(([value, label]) => (
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
          </FormItem>
        )}
      />
      <SwitchField
        form={form}
        name="uses_non_exempt_encryption"
        label="Uses non-exempt encryption"
        description="Oore answers Apple’s export compliance question with this on every build. Leave it off if the app only uses encryption built into iOS, such as HTTPS."
      />
    </AdvancedOptions>
  )
}

function AppStorePublishForm({
  appKeyName,
  canWrite,
  file,
  pipelineId,
  projectId,
  saved,
}: {
  appKeyName: string | undefined
  canWrite: boolean
  file: FilePublishing | null
  pipelineId: string
  projectId: string
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
  const connected = !!appKeyName || saved.has_api_key

  function onSubmit(next: FormValues) {
    updateMutation.mutate(toConfig(next, saved.config), {
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
          <SettingsSurface className="flex flex-col gap-4">
            {file?.all ? (
              <FileControlledLine
                file={file}
                pipelineId={pipelineId}
                projectId={projectId}
              />
            ) : connected ? (
              <DestinationLine form={form} />
            ) : (
              <NotConnectedLine
                projectId={projectId}
                store="App Store Connect"
              />
            )}
            {file && !file.all ? (
              <PartlyFileControlledNote
                file={file}
                pipelineId={pipelineId}
                projectId={projectId}
              />
            ) : null}

            {(enabled || file?.all) && connected ? (
              <AdvancedFields appKeyName={appKeyName} form={form} />
            ) : null}

            {updateMutation.error ? (
              <Alert variant="destructive">
                <HugeiconsIcon icon={InformationCircleIcon} />
                <AlertTitle>Settings were not saved</AlertTitle>
                <AlertDescription>
                  {getApiErrorMessage(updateMutation.error, {})}
                </AlertDescription>
              </Alert>
            ) : null}

            {canWrite && isDirty ? (
              <div className="flex justify-end gap-2">
                {!isNew ? (
                  <Button
                    type="button"
                    variant="outline"
                    onClick={() => form.reset(toFormValues(saved.config))}
                  >
                    Discard changes
                  </Button>
                ) : null}
                <Button type="submit" disabled={updateMutation.isPending}>
                  {updateMutation.isPending ? (
                    <Spinner data-icon="inline-start" />
                  ) : null}
                  Save changes
                </Button>
              </div>
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
  const storesQuery = useProjectStores(projectId)
  const file = useFilePublishing(pipelineId, 'ios')
  const data = publishQuery.data
  const appKey = storesQuery.data?.app_store?.key
  const appKeyName = appKey ? `${appKey.name} (${appKey.key_id})` : undefined
  const uploadsOn = !!data?.config.enabled || !!file
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
  // Only what blocks an upload is shown; a ready pipeline is one line.
  const problems: Array<ReadinessCheck> = data
    ? [
        ...(data.app_store_export
          ? []
          : [
              {
                key: 'distribution',
                label: 'App Store distribution',
                ready: false,
                detail:
                  'iOS signing must use App Store distribution. Ad hoc builds cannot go to TestFlight or the App Store.',
                action: editLink('Edit iOS signing'),
              },
            ]),
        ...(data.has_api_key
          ? []
          : [
              {
                key: 'key',
                label: 'App Store Connect API key',
                ready: false,
                detail:
                  'Connect App Store Connect in the app’s Stores, or add a key in iOS signing.',
              },
            ]),
        ...(storeReadiness.data?.items ?? [])
          .filter(
            (item) =>
              item.store === 'app_store' &&
              item.code !== 'api_key' &&
              item.level !== 'ok',
          )
          .map((item): ReadinessCheck => ({
            key: `store-${item.code}`,
            label: item.remembered
              ? `${item.title} (Apple’s last refusal)`
              : item.title,
            ready: false,
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

  return (
    <SettingsSection id="app-store" title="App Store Connect">
      {uploadsOn && problems.length > 0 ? (
        <PublishReadiness checks={problems} />
      ) : null}

      {publishQuery.error ? (
        <CollectionError
          title="Failed to load App Store settings"
          description={publishQuery.error.message}
          onRetry={() => void publishQuery.refetch()}
        />
      ) : data && !storesQuery.isLoading ? (
        <AppStorePublishForm
          appKeyName={appKeyName}
          canWrite={canWrite}
          file={file}
          pipelineId={pipelineId}
          projectId={projectId}
          saved={data}
        />
      ) : (
        <Skeleton className="h-16 w-full" />
      )}
    </SettingsSection>
  )
}
