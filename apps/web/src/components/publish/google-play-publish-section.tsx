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
import type { PipelineAndroidSigningResponse } from '@oore/client/models'

import { getApiErrorMessage } from '@/lib/api-client/api-error'
import { buildsAppBundle } from '@/lib/pipeline-form-utils'
import { PLAY_TRACKS } from '@/lib/store-destination'
import { toast } from '@/lib/toast'
import {
  usePipelineGooglePlayPublish,
  useUpdatePipelineGooglePlayPublish,
  type GooglePlayPublishConfig,
  type GooglePlayPublishResponse,
} from '@/hooks/use-pipeline-publish'
import {
  useInheritedSecrets,
  type SecretScope,
  type SecretTarget,
} from '@/hooks/use-secrets'
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
import { PLAY_SERVICE_ACCOUNT_SECRET } from '@/components/store-credentials/google-play-service-account-panel'
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
import { Stack } from '@/components/layout/stack'

const TRACKS = {
  ...PLAY_TRACKS,
  custom: {
    label: 'Custom track',
    effect: 'A custom closed testing track you created in Play Console.',
  },
} as const

type TrackChoice = keyof typeof TRACKS
const RELEASE_TYPES = {
  full: 'Full rollout',
  staged: 'Staged rollout',
  draft: 'Draft',
} as const

// "none" is no longer offered: a version without notes sends none anyway.
const RELEASE_NOTES = {
  changelog: 'The version’s release notes',
  repo_files: 'Files in the repository',
} as const

const PRIORITIES = {
  none: 'Not set',
  '0': '0 (default)',
  '1': '1',
  '2': '2',
  '3': '3',
  '4': '4',
  '5': '5 (highest)',
} as const

const PRIORITY_VALUES = z.enum(['none', '0', '1', '2', '3', '4', '5'])

const TRACK_CHOICE = z.enum([
  'internal',
  'closed',
  'open',
  'production',
  'custom',
])

const formSchema = z
  .object({
    enabled: z.boolean(),
    // Empty uses the app's Google Play connection.
    service_account_secret: z.string(),
    track: TRACK_CHOICE,
    custom_track: z.string().trim(),
    release_type: z.enum(['full', 'staged', 'draft']),
    rollout_percent: z.string().trim(),
    release_notes: z.enum(['none', 'changelog', 'repo_files']),
    release_notes_locale: z.string().trim(),
    release_notes_dir: z.string().trim(),
    listing_dir: z.string().trim(),
    package_name: z.string().trim(),
    upload_mapping: z.boolean(),
    in_app_update_priority: PRIORITY_VALUES,
    preflight: z.enum(['auto', 'block', 'warn', 'off']),
  })
  .superRefine((values, ctx) => {
    if (
      values.track === 'custom' &&
      !/^[A-Za-z0-9_:-]{1,128}$/.test(values.custom_track)
    ) {
      ctx.addIssue({
        code: 'custom',
        path: ['custom_track'],
        message: 'Enter the track name as it appears in Play Console.',
      })
    }
    if (values.release_type === 'staged') {
      const percent = Number(values.rollout_percent)
      if (!(percent > 0 && percent < 100)) {
        ctx.addIssue({
          code: 'custom',
          path: ['rollout_percent'],
          message: 'Enter a share above 0 and below 100.',
        })
      }
    }
    if (
      values.release_notes === 'changelog' &&
      !/^[A-Za-z]{2,3}(-[A-Za-z0-9]{2,8})*$/.test(values.release_notes_locale)
    ) {
      ctx.addIssue({
        code: 'custom',
        path: ['release_notes_locale'],
        message: 'Enter a Play language code such as en-US.',
      })
    }
    if (values.release_notes === 'repo_files' && !values.release_notes_dir) {
      ctx.addIssue({
        code: 'custom',
        path: ['release_notes_dir'],
        message:
          'Enter the repository directory that holds <locale>.txt files.',
      })
    }
    if (
      values.listing_dir.startsWith('/') ||
      values.listing_dir.split('/').includes('..')
    ) {
      ctx.addIssue({
        code: 'custom',
        path: ['listing_dir'],
        message:
          'Use a path inside the repository, such as fastlane/metadata/android.',
      })
    }
    if (
      values.package_name &&
      !/^[A-Za-z][A-Za-z0-9_]*(\.[A-Za-z][A-Za-z0-9_]*)+$/.test(
        values.package_name,
      )
    ) {
      ctx.addIssue({
        code: 'custom',
        path: ['package_name'],
        message: 'Enter an application id such as com.example.app.',
      })
    }
  })

type FormValues = z.infer<typeof formSchema>

function isTrackChoice(track: string): track is Exclude<TrackChoice, 'custom'> {
  return track in TRACKS && track !== 'custom'
}

/**
 * `defaultSecret` is preselected for new settings, so a service account
 * shared across projects needs no choice.
 */
function toFormValues(
  config: GooglePlayPublishConfig | null | undefined,
  defaultSecret = '',
) {
  const track = config?.track ?? 'internal'
  const percent = config?.rollout_percent
  return {
    enabled: config?.enabled ?? false,
    service_account_secret: config
      ? config.service_account_secret
      : defaultSecret,
    track: isTrackChoice(track) ? track : 'custom',
    custom_track: isTrackChoice(track) ? '' : track,
    release_type: config?.draft
      ? 'draft'
      : percent != null && percent < 100
        ? 'staged'
        : 'full',
    rollout_percent: percent != null && percent < 100 ? String(percent) : '5',
    release_notes:
      config?.release_notes === 'repo_files' ? 'repo_files' : 'changelog',
    release_notes_locale: config?.release_notes_locale ?? 'en-US',
    release_notes_dir: config?.release_notes_dir ?? '',
    listing_dir: config?.listing_dir ?? '',
    package_name: config?.package_name ?? '',
    upload_mapping: config?.upload_mapping ?? true,
    in_app_update_priority:
      PRIORITY_VALUES.safeParse(String(config?.in_app_update_priority)).data ??
      'none',
    preflight: config?.preflight ?? 'auto',
  } satisfies FormValues
}

function toConfig(values: FormValues): GooglePlayPublishConfig {
  return {
    enabled: values.enabled,
    service_account_secret: values.service_account_secret,
    track: values.track === 'custom' ? values.custom_track : values.track,
    rollout_percent:
      values.release_type === 'staged' ? Number(values.rollout_percent) : null,
    draft: values.release_type === 'draft',
    release_notes: values.release_notes,
    release_notes_locale: values.release_notes_locale || 'en-US',
    release_notes_dir:
      values.release_notes === 'repo_files' ? values.release_notes_dir : null,
    listing_dir: values.listing_dir || null,
    package_name: values.package_name || null,
    upload_mapping: values.upload_mapping,
    in_app_update_priority:
      values.in_app_update_priority === 'none'
        ? null
        : Number(values.in_app_update_priority),
    preflight: values.preflight,
  }
}

const PREFLIGHT_LEVELS = {
  auto: 'Automatic',
  block: 'Stop the upload',
  warn: 'Warn only',
  off: 'Off',
} as const

const APP_SECRET = '__app__'

const TRACK_LINE_ITEMS = {
  off: 'Nowhere',
  ...Object.fromEntries(
    Object.entries(TRACKS).map(([value, track]) => [value, track.label]),
  ),
}

/** "After a successful build, send to" and the track list, one line. */
function TrackLine({ form }: { form: UseFormReturn<FormValues> }) {
  const [enabled, track] = useWatch({
    control: form.control,
    name: ['enabled', 'track'],
  })
  const value = enabled ? track : 'off'
  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
        <label htmlFor="google-play-track" className="text-sm">
          After a successful build, send to
        </label>
        <Select
          value={value}
          onValueChange={(next) => {
            if (!next || next === 'off') {
              form.setValue('enabled', false, { shouldDirty: true })
              return
            }
            const choice = TRACK_CHOICE.safeParse(next).data
            if (!choice) return
            form.setValue('enabled', true, { shouldDirty: true })
            form.setValue('track', choice, { shouldDirty: true })
            if (
              choice === 'internal' &&
              form.getValues('release_type') === 'staged'
            ) {
              form.setValue('release_type', 'full', { shouldDirty: true })
            }
          }}
          items={TRACK_LINE_ITEMS}
        >
          <SelectTrigger id="google-play-track" className="min-w-48">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {Object.entries(TRACK_LINE_ITEMS).map(([key, label]) => (
              <SelectItem key={key} value={key}>
                {label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        {enabled ? (
          <InfoHint label="Track">{TRACKS[track].effect}</InfoHint>
        ) : null}
      </div>
      {enabled && track === 'custom' ? (
        <FormField
          control={form.control}
          name="custom_track"
          render={({ field }) => (
            <FormItem className="max-w-80">
              <FormLabel>Track name</FormLabel>
              <FormControl>
                <Input
                  className="font-mono"
                  placeholder="qa-team"
                  autoComplete="off"
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

/**
 * Which key uploads: the app's Google Play connection by default, or a
 * secret this pipeline names itself.
 */
function ServiceAccountField({
  appSecret,
  form,
  saved,
  secretScopes,
}: {
  appSecret: string | undefined
  form: UseFormReturn<FormValues>
  saved: GooglePlayPublishResponse | undefined
  secretScopes: Map<string, SecretScope>
}) {
  const value = useWatch({
    control: form.control,
    name: 'service_account_secret',
  })
  const options = new Map<string, string>()
  if (appSecret) options.set(APP_SECRET, `The app’s key (${appSecret})`)
  for (const name of secretScopes.keys()) options.set(name, name)
  if (value && !options.has(value)) options.set(value, value)
  const scope = secretScopes.get(value)
  const savedPresent =
    saved?.config?.service_account_secret === value &&
    saved.service_account_secret_present
  const items = Object.fromEntries(options)

  return (
    <FormField
      control={form.control}
      name="service_account_secret"
      render={({ field }) => (
        <FormItem>
          <FormLabel hint="A secret named here replaces the app’s Google Play key for this pipeline only.">
            Service account key
          </FormLabel>
          <Select
            value={field.value || (appSecret ? APP_SECRET : null)}
            onValueChange={(next) =>
              field.onChange(!next || next === APP_SECRET ? '' : next)
            }
            items={items}
          >
            <FormControl>
              <SelectTrigger className="w-full">
                <SelectValue placeholder="Choose a secret" />
              </SelectTrigger>
            </FormControl>
            <SelectContent>
              {[...options].map(([key, label]) => (
                <SelectItem
                  key={key}
                  value={key}
                  className={key === APP_SECRET ? undefined : 'font-mono'}
                >
                  {label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          {value && !scope && !savedPresent ? (
            <p className="text-sm text-destructive">
              No secret named {value} is visible to this pipeline. The upload
              step will fail until you add it.
            </p>
          ) : null}
          <FormMessage />
        </FormItem>
      )}
    />
  )
}

function ReleaseFields({ form }: { form: UseFormReturn<FormValues> }) {
  const [track, releaseType, releaseNotes] = useWatch({
    control: form.control,
    name: ['track', 'release_type', 'release_notes'],
  })

  return (
    <>
      <FormField
        control={form.control}
        name="release_type"
        render={({ field }) => (
          <FormItem>
            <FormLabel
              hint={
                releaseType === 'draft'
                  ? 'Nothing reaches users until you review and roll out the draft in Play Console.'
                  : releaseType === 'staged'
                    ? 'Only the share below gets the update at first. Increase, halt or complete it later from the release board or Play Console.'
                    : track === 'internal'
                      ? 'Every internal tester gets the update. The internal track does not support staged rollouts.'
                      : 'Everyone on the track gets the update as soon as Google approves it.'
              }
            >
              Rollout
            </FormLabel>
            <FormControl>
              <RadioGroup
                value={field.value}
                onValueChange={field.onChange}
                className="flex flex-wrap gap-x-6 gap-y-2"
              >
                {Object.entries(RELEASE_TYPES).map(([value, label]) => {
                  const disabled = value === 'staged' && track === 'internal'
                  return (
                    <label
                      key={value}
                      className="flex items-center gap-2 text-sm has-disabled:opacity-50"
                    >
                      <RadioGroupItem value={value} disabled={disabled} />
                      {label}
                    </label>
                  )
                })}
              </RadioGroup>
            </FormControl>
            <FormMessage />
          </FormItem>
        )}
      />

      {releaseType === 'staged' ? (
        <FormField
          control={form.control}
          name="rollout_percent"
          render={({ field }) => (
            <FormItem className="max-w-48">
              <FormLabel>Initial rollout (%)</FormLabel>
              <FormControl>
                <Input
                  type="number"
                  inputMode="decimal"
                  step="any"
                  {...field}
                />
              </FormControl>
              <FormMessage />
            </FormItem>
          )}
        />
      ) : null}

      <Stack gap={4}>
        <FormField
          control={form.control}
          name="release_notes"
          render={({ field }) => (
            <FormItem>
              <FormLabel
                hint={
                  field.value === 'repo_files'
                    ? 'One <locale>.txt file per language, such as en-US.txt.'
                    : 'Written once per app version on the build page, or read from a changelog in .oore.yaml. Play shows the first 500 characters; a version without notes sends none.'
                }
              >
                Release notes
              </FormLabel>
              <Select
                value={field.value}
                onValueChange={field.onChange}
                items={RELEASE_NOTES}
              >
                <FormControl>
                  <SelectTrigger className="w-full">
                    <SelectValue />
                  </SelectTrigger>
                </FormControl>
                <SelectContent>
                  {Object.entries(RELEASE_NOTES).map(([value, label]) => (
                    <SelectItem key={value} value={value}>
                      {label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
              <FormMessage />
            </FormItem>
          )}
        />
        {releaseNotes === 'changelog' ? (
          <FormField
            control={form.control}
            name="release_notes_locale"
            render={({ field }) => (
              <FormItem>
                <FormLabel>Language of the notes</FormLabel>
                <FormControl>
                  <Input className="font-mono" placeholder="en-US" {...field} />
                </FormControl>
                <FormMessage />
              </FormItem>
            )}
          />
        ) : releaseNotes === 'repo_files' ? (
          <FormField
            control={form.control}
            name="release_notes_dir"
            render={({ field }) => (
              <FormItem>
                <FormLabel>Directory</FormLabel>
                <FormControl>
                  <Input
                    className="font-mono"
                    placeholder="release-notes"
                    {...field}
                  />
                </FormControl>
                <FormMessage />
              </FormItem>
            )}
          />
        ) : null}
      </Stack>
    </>
  )
}

const ADVANCED_FIELDS = [
  'release_type',
  'rollout_percent',
  'release_notes',
  'release_notes_locale',
  'release_notes_dir',
  'service_account_secret',
  'listing_dir',
  'package_name',
  'in_app_update_priority',
  'upload_mapping',
  'preflight',
] as const

/** What differs from the defaults, for the closed fold. */
function advancedSummary(values: {
  release_type: FormValues['release_type']
  rollout_percent: string
  release_notes: FormValues['release_notes']
  service_account_secret: string
  package_name: string
  listing_dir: string
}) {
  const parts = [
    values.release_type === 'staged'
      ? `${values.rollout_percent || '?'}% staged rollout`
      : values.release_type === 'draft'
        ? 'Draft'
        : 'Full rollout',
    values.release_notes === 'repo_files' ? 'notes from files' : null,
    values.service_account_secret
      ? `key ${values.service_account_secret}`
      : null,
    values.package_name ? values.package_name : null,
    values.listing_dir ? 'store listing' : null,
  ]
  return parts.filter(Boolean).join(' · ')
}

function AdvancedFields({
  appSecret,
  form,
  saved,
  secretScopes,
}: {
  appSecret: string | undefined
  form: UseFormReturn<FormValues>
  saved: GooglePlayPublishResponse
  secretScopes: Map<string, SecretScope>
}) {
  const [
    releaseType,
    rolloutPercent,
    releaseNotes,
    ,
    ,
    secret,
    listingDir,
    packageName,
  ] = useWatch({ control: form.control, name: ADVANCED_FIELDS })
  const { errors } = useFormState({ control: form.control })
  return (
    <AdvancedOptions
      changed={false}
      hasError={ADVANCED_FIELDS.some((name) => !!errors[name])}
      summary={advancedSummary({
        release_type: releaseType,
        rollout_percent: rolloutPercent,
        release_notes: releaseNotes,
        service_account_secret: secret,
        package_name: packageName,
        listing_dir: listingDir,
      })}
    >
      <ReleaseFields form={form} />
      <ServiceAccountField
        appSecret={appSecret}
        form={form}
        saved={saved}
        secretScopes={secretScopes}
      />
      <FormField
        control={form.control}
        name="listing_dir"
        render={({ field }) => (
          <FormItem>
            <FormLabel
              hint={
                'fastlane’s layout: en-US/title.txt, short_description.txt, full_description.txt and video.txt. Sent with every upload; files you leave out stay as they are in Play Console. Screenshots are not uploaded.'
              }
            >
              Store listing from the repository
            </FormLabel>
            <FormControl>
              <Input
                className="font-mono"
                placeholder="fastlane/metadata/android"
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
        name="package_name"
        render={({ field }) => (
          <FormItem>
            <FormLabel
              hint={
                'When set, a bundle for any other application id fails the upload. Empty uses the app’s package.'
              }
            >
              Expected package
            </FormLabel>
            <FormControl>
              <Input
                className="font-mono"
                placeholder="The app’s package"
                autoComplete="off"
                {...field}
              />
            </FormControl>
            <FormMessage />
          </FormItem>
        )}
      />
      <FormField
        control={form.control}
        name="in_app_update_priority"
        render={({ field }) => (
          <FormItem>
            <FormLabel
              hint={
                'Apps using the Play in-app updates API can prompt harder for higher priorities.'
              }
            >
              In-app update priority
            </FormLabel>
            <Select
              value={field.value}
              onValueChange={field.onChange}
              items={PRIORITIES}
            >
              <FormControl>
                <SelectTrigger className="w-full">
                  <SelectValue />
                </SelectTrigger>
              </FormControl>
              <SelectContent>
                {Object.entries(PRIORITIES).map(([value, label]) => (
                  <SelectItem key={value} value={value}>
                    {label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            <FormMessage />
          </FormItem>
        )}
      />
      <FormField
        control={form.control}
        name="upload_mapping"
        render={({ field }) => (
          <FormItem className="flex flex-row items-start justify-between gap-4">
            <Stack gap={1}>
              <FormLabel
                hint={
                  'Play Console deobfuscates crash reports with it. Skipped when the build produced none.'
                }
              >
                Upload mapping.txt
              </FormLabel>
            </Stack>
            <FormControl>
              <Switch
                checked={field.value}
                onCheckedChange={field.onChange}
                aria-label="Upload the R8 mapping file"
              />
            </FormControl>
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
                'Before the release is committed, Google Play validates it, which catches a version code Play already has or a track the service account cannot release to. Automatic warns until a release from this pipeline has been accepted, then stops a refused release so nothing half-published reaches Play.'
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
    </AdvancedOptions>
  )
}

function GooglePlayPublishForm({
  appSecret,
  canWrite,
  file,
  pipelineId,
  projectId,
  saved,
  secretScopes,
}: {
  appSecret: string | undefined
  canWrite: boolean
  file: FilePublishing | null
  pipelineId: string
  projectId: string
  saved: GooglePlayPublishResponse
  secretScopes: Map<string, SecretScope>
}) {
  const updateMutation = useUpdatePipelineGooglePlayPublish(pipelineId)
  // A new pipeline follows the app's key; without one, the shared secret.
  const defaultSecret =
    appSecret || !secretScopes.has(PLAY_SERVICE_ACCOUNT_SECRET)
      ? ''
      : PLAY_SERVICE_ACCOUNT_SECRET
  const form = useForm<FormValues>({
    resolver: zodResolver(formSchema),
    values: toFormValues(saved.config, defaultSecret),
    resetOptions: { keepDirtyValues: true },
  })
  const { isDirty } = useFormState({ control: form.control })
  const [enabled, secret] = useWatch({
    control: form.control,
    name: ['enabled', 'service_account_secret'],
  })
  const isNew = !saved.config
  // New settings start off; picking a track is the change to save.
  const canSave = isDirty
  const connected = !!appSecret || !!saved.config?.service_account_secret
  const missingKey = enabled && !secret && !appSecret

  function onSubmit(next: FormValues) {
    if (next.enabled && !next.service_account_secret && !appSecret) {
      form.setError('service_account_secret', {
        message: 'Connect Google Play in the app’s Stores, or name a secret.',
      })
      return
    }
    updateMutation.mutate(toConfig(next), {
      onSuccess: (response) => {
        form.reset(toFormValues(response.config))
        toast.success('Google Play settings saved')
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
              <TrackLine form={form} />
            ) : (
              <NotConnectedLine projectId={projectId} store="Google Play" />
            )}
            {file && !file.all ? (
              <PartlyFileControlledNote
                file={file}
                pipelineId={pipelineId}
                projectId={projectId}
              />
            ) : null}

            {(enabled || file?.all) && connected ? (
              <AdvancedFields
                appSecret={appSecret}
                form={form}
                saved={saved}
                secretScopes={secretScopes}
              />
            ) : null}

            {missingKey ? (
              <p className="text-sm text-destructive">
                Connect Google Play in the app’s Stores, or name a secret under
                Advanced.
              </p>
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
                <Button
                  type="submit"
                  disabled={updateMutation.isPending || !canSave}
                >
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
        title="Discard Google Play changes?"
        description="Your changes to the Google Play settings have not been saved."
      />
    </Form>
  )
}

export function GooglePlayPublishSection({
  androidCommandOverride,
  androidSigning,
  canReadInstanceSecrets,
  canWrite,
  pipelineId,
  projectId,
}: {
  androidCommandOverride: string | null | undefined
  androidSigning: PipelineAndroidSigningResponse | undefined
  canReadInstanceSecrets: boolean
  canWrite: boolean
  pipelineId: string
  projectId: string
}) {
  const publishQuery = usePipelineGooglePlayPublish(pipelineId)
  const storesQuery = useProjectStores(projectId)
  const file = useFilePublishing(pipelineId, 'android')
  const saved = publishQuery.data
  const appSecret = storesQuery.data?.google_play?.service_account_secret
  const secretTargets: Array<SecretTarget> = [
    { scope: 'pipeline', pipelineId },
    { scope: 'project', projectId },
    ...(canReadInstanceSecrets ? [{ scope: 'instance' as const }] : []),
  ]
  const visibleSecrets = useInheritedSecrets(secretTargets)
  // Narrowest scope first, so the first entry per name is the one a build uses.
  const secretScopes = new Map<string, SecretScope>()
  for (const secret of visibleSecrets) {
    if (!secretScopes.has(secret.name)) {
      secretScopes.set(secret.name, secret.scope)
    }
  }
  const uploadsOn = !!saved?.config?.enabled || !!file
  const ownSecret = saved?.config?.service_account_secret
  const secretName = ownSecret || appSecret || ''
  const secretReady =
    !!secretName &&
    (secretScopes.has(secretName) || !!saved?.service_account_secret_present)
  const override = androidCommandOverride?.trim() ?? ''
  const bundleReady = !override || buildsAppBundle(override)
  const editLink = (hash: string, label: string) =>
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
            hash={hash}
          />
        }
      >
        {label}
      </Button>
    ) : undefined
  // Only what blocks an upload is shown; a ready pipeline is one line.
  const problems: Array<ReadinessCheck> = []
  if (
    androidSigning &&
    !(androidSigning.release.enabled && androidSigning.release.has_keystore)
  ) {
    problems.push({
      key: 'signing',
      label: 'Release signing',
      ready: false,
      detail:
        'Google Play rejects bundles that are not signed with your upload key.',
      action: editLink('android-signing', 'Set up signing'),
    })
  }
  if (saved?.config && secretName && !secretReady) {
    problems.push({
      key: 'key',
      label: 'Service account key',
      ready: false,
      detail: `No secret named ${secretName} is set. Uploads fail until you add it.`,
    })
  }
  if (!bundleReady) {
    problems.push({
      key: 'bundle',
      label: 'App Bundle build',
      ready: false,
      detail:
        'Your Android command override builds no App Bundle, so the upload fails. Use flutter build appbundle or clear the override.',
      action: editLink('build', 'Edit build'),
    })
  }

  return (
    <SettingsSection id="google-play" title="Google Play">
      {uploadsOn && problems.length > 0 ? (
        <PublishReadiness checks={problems} />
      ) : null}

      {publishQuery.error ? (
        <CollectionError
          title="Failed to load Google Play settings"
          description={publishQuery.error.message}
          onRetry={() => void publishQuery.refetch()}
        />
      ) : saved && !storesQuery.isLoading ? (
        <GooglePlayPublishForm
          appSecret={appSecret}
          canWrite={canWrite}
          file={file}
          pipelineId={pipelineId}
          projectId={projectId}
          saved={saved}
          secretScopes={secretScopes}
        />
      ) : (
        <Skeleton className="h-16 w-full" />
      )}
    </SettingsSection>
  )
}
