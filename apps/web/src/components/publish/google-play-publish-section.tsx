import { useState } from 'react'
import {
  useForm,
  useFormState,
  useWatch,
  type UseFormReturn,
} from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import * as z from 'zod'
import { HugeiconsIcon } from '@hugeicons/react'
import { Add01Icon, InformationCircleIcon } from '@hugeicons/core-free-icons'
import type { PipelineAndroidSigningResponse } from '@oore/client/models'

import { getApiErrorMessage } from '@/lib/api-client/api-error'
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
import { SecretValueDialog } from '@/components/secrets/secret-value-dialog'
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

const TRACKS = {
  internal: {
    label: 'Internal testing',
    effect:
      'Up to 100 testers you list in Play Console get it within minutes. Staged rollouts are not available.',
  },
  closed: {
    label: 'Closed testing',
    effect:
      'Testers you invite by email or Google Group get it (Play’s alpha track).',
  },
  open: {
    label: 'Open testing',
    effect:
      'Anyone can join the test from your Play Store listing (Play’s beta track).',
  },
  production: {
    label: 'Production',
    effect: 'Everyone on Google Play gets it once Google approves the release.',
  },
  custom: {
    label: 'Custom track',
    effect: 'A custom closed testing track you created in Play Console.',
  },
} as const

type TrackChoice = keyof typeof TRACKS
const TRACK_LABELS = Object.fromEntries(
  Object.entries(TRACKS).map(([value, track]) => [value, track.label]),
)

const RELEASE_TYPES = {
  full: 'Full rollout',
  staged: 'Staged rollout',
  draft: 'Draft',
} as const

const RELEASE_NOTES = {
  none: 'None',
  changelog: 'Build changelog',
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

const SCOPE_LABEL = {
  instance: 'instance',
  project: 'project',
  pipeline: 'pipeline',
} as const satisfies Record<SecretScope, string>

const formSchema = z
  .object({
    enabled: z.boolean(),
    service_account_secret: z
      .string()
      .min(1, 'Choose the secret that holds the service account JSON key.'),
    track: z.enum(['internal', 'closed', 'open', 'production', 'custom']),
    custom_track: z.string().trim(),
    release_type: z.enum(['full', 'staged', 'draft']),
    rollout_percent: z.string().trim(),
    release_notes: z.enum(['none', 'changelog', 'repo_files']),
    release_notes_locale: z.string().trim(),
    release_notes_dir: z.string().trim(),
    package_name: z.string().trim(),
    upload_mapping: z.boolean(),
    in_app_update_priority: PRIORITY_VALUES,
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

function toFormValues(config: GooglePlayPublishConfig | null | undefined) {
  const track = config?.track ?? 'internal'
  const percent = config?.rollout_percent
  return {
    enabled: config?.enabled ?? true,
    service_account_secret: config?.service_account_secret ?? '',
    track: isTrackChoice(track) ? track : 'custom',
    custom_track: isTrackChoice(track) ? '' : track,
    release_type: config?.draft
      ? 'draft'
      : percent != null && percent < 100
        ? 'staged'
        : 'full',
    rollout_percent: percent != null && percent < 100 ? String(percent) : '5',
    release_notes: config?.release_notes ?? 'changelog',
    release_notes_locale: config?.release_notes_locale ?? 'en-US',
    release_notes_dir: config?.release_notes_dir ?? '',
    package_name: config?.package_name ?? '',
    upload_mapping: config?.upload_mapping ?? true,
    in_app_update_priority:
      PRIORITY_VALUES.safeParse(String(config?.in_app_update_priority)).data ??
      'none',
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
    package_name: values.package_name || null,
    upload_mapping: values.upload_mapping,
    in_app_update_priority:
      values.in_app_update_priority === 'none'
        ? null
        : Number(values.in_app_update_priority),
  }
}

/** One sentence describing what the saved settings will do to a build. */
const EFFECT_FIELDS = [
  'enabled',
  'track',
  'custom_track',
  'release_type',
  'rollout_percent',
  'release_notes',
  'release_notes_locale',
  'release_notes_dir',
] as const

type EffectValues = Pick<FormValues, (typeof EFFECT_FIELDS)[number]>

function describeEffect(values: EffectValues) {
  if (!values.enabled) {
    return 'Builds of this pipeline are not uploaded to Google Play.'
  }
  const track =
    values.track === 'custom'
      ? `the ${values.custom_track || 'custom'} track`
      : `the ${TRACKS[values.track].label} track`
  const release =
    values.release_type === 'draft'
      ? 'as a draft you publish from Play Console'
      : values.release_type === 'staged'
        ? `and rolled out to ${values.rollout_percent || '?'}% of users`
        : 'and released to everyone on that track'
  const notes =
    values.release_notes === 'changelog'
      ? ` The build changelog becomes the ${values.release_notes_locale || 'en-US'} release notes.`
      : values.release_notes === 'repo_files'
        ? ` Release notes come from ${values.release_notes_dir || 'a repository directory'}.`
        : ''
  return `After each successful build, the signed App Bundle is uploaded to ${track} ${release}.${notes}`
}

function UploadToggle({ form }: { form: UseFormReturn<FormValues> }) {
  const [
    enabled,
    track,
    custom_track,
    release_type,
    rollout_percent,
    release_notes,
    release_notes_locale,
    release_notes_dir,
  ] = useWatch({ control: form.control, name: EFFECT_FIELDS })
  const effect = describeEffect({
    enabled,
    track,
    custom_track,
    release_type,
    rollout_percent,
    release_notes,
    release_notes_locale,
    release_notes_dir,
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
                  aria-label="Upload to Google Play after each successful build"
                />
              </FormControl>
            </FormItem>
          }
        />
      )}
    />
  )
}

function ServiceAccountField({
  form,
  onAddSecret,
  saved,
  secretScopes,
}: {
  form: UseFormReturn<FormValues>
  onAddSecret: () => void
  saved: GooglePlayPublishResponse | undefined
  secretScopes: Map<string, SecretScope>
}) {
  const value = useWatch({
    control: form.control,
    name: 'service_account_secret',
  })
  const options = new Map(secretScopes)
  if (value && !options.has(value)) options.set(value, 'pipeline')
  const scope = secretScopes.get(value)
  const savedPresent =
    saved?.config?.service_account_secret === value &&
    saved.service_account_secret_present
  const items = Object.fromEntries(
    [...options.keys()].map((name) => [name, name]),
  )

  return (
    <FormField
      control={form.control}
      name="service_account_secret"
      render={({ field }) => (
        <FormItem>
          <FormLabel>Service account key</FormLabel>
          <div className="flex flex-wrap items-center gap-2">
            <Select
              value={field.value || null}
              onValueChange={(next) => field.onChange(next ?? '')}
              items={items}
            >
              <FormControl>
                <SelectTrigger className="min-w-56 font-mono">
                  <SelectValue
                    placeholder={
                      options.size > 0 ? 'Choose a secret' : 'No secrets yet'
                    }
                  />
                </SelectTrigger>
              </FormControl>
              <SelectContent>
                {[...options.keys()].map((name) => (
                  <SelectItem key={name} value={name} className="font-mono">
                    {name}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={onAddSecret}
            >
              <HugeiconsIcon icon={Add01Icon} />
              Add secret
            </Button>
          </div>
          <FormDescription>
            {!value
              ? 'Store the whole service account JSON key file as a secret, then choose it here.'
              : scope
                ? `Uses the ${SCOPE_LABEL[scope]} secret ${value}. The key stays on the runner that builds the app.`
                : savedPresent
                  ? `Uses ${value} from a scope you can’t list.`
                  : `No secret named ${value} is visible to this pipeline. The upload step will fail until you add it.`}
          </FormDescription>
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
      <div className="grid gap-4 sm:grid-cols-2">
        <FormField
          control={form.control}
          name="track"
          render={({ field }) => (
            <FormItem>
              <FormLabel>Track</FormLabel>
              <Select
                value={field.value}
                onValueChange={(next) => {
                  field.onChange(next)
                  if (
                    next === 'internal' &&
                    form.getValues('release_type') === 'staged'
                  ) {
                    form.setValue('release_type', 'full')
                  }
                }}
                items={TRACK_LABELS}
              >
                <FormControl>
                  <SelectTrigger className="w-full">
                    <SelectValue />
                  </SelectTrigger>
                </FormControl>
                <SelectContent>
                  {Object.entries(TRACKS).map(([value, option]) => (
                    <SelectItem key={value} value={value}>
                      {option.label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
              <FormDescription>{TRACKS[field.value].effect}</FormDescription>
              <FormMessage />
            </FormItem>
          )}
        />
        {track === 'custom' ? (
          <FormField
            control={form.control}
            name="custom_track"
            render={({ field }) => (
              <FormItem>
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

      <FormField
        control={form.control}
        name="release_type"
        render={({ field }) => (
          <FormItem>
            <FormLabel>Release</FormLabel>
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
            <FormDescription>
              {releaseType === 'draft'
                ? 'Nothing reaches users until you review and roll out the draft in Play Console.'
                : releaseType === 'staged'
                  ? 'Only the share below gets the update at first. Raise, halt or complete it later from the release board or Play Console.'
                  : track === 'internal'
                    ? 'Every internal tester gets the update. The internal track does not support staged rollouts.'
                    : 'Everyone on the track gets the update as soon as Google approves it.'}
            </FormDescription>
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
                  min={0.01}
                  max={99.99}
                  step="any"
                  {...field}
                />
              </FormControl>
              <FormMessage />
            </FormItem>
          )}
        />
      ) : null}

      <div className="grid gap-4 sm:grid-cols-2">
        <FormField
          control={form.control}
          name="release_notes"
          render={({ field }) => (
            <FormItem>
              <FormLabel>Release notes</FormLabel>
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
              <FormDescription>
                {field.value === 'none'
                  ? 'Users see no “What’s new” text for this release.'
                  : field.value === 'changelog'
                    ? 'The build changelog, shortened to Play’s 500-character limit.'
                    : 'One <locale>.txt file per language, such as en-US.txt.'}
              </FormDescription>
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
                <FormLabel>Language</FormLabel>
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
      </div>
    </>
  )
}

function AdvancedFields({ form }: { form: UseFormReturn<FormValues> }) {
  return (
    <div className="grid gap-4 sm:grid-cols-2">
      <FormField
        control={form.control}
        name="package_name"
        render={({ field }) => (
          <FormItem>
            <FormLabel>Expected package</FormLabel>
            <FormControl>
              <Input
                className="font-mono"
                placeholder="Read from the App Bundle"
                autoComplete="off"
                {...field}
              />
            </FormControl>
            <FormDescription>
              When set, a bundle for any other application id fails the upload.
            </FormDescription>
            <FormMessage />
          </FormItem>
        )}
      />
      <FormField
        control={form.control}
        name="in_app_update_priority"
        render={({ field }) => (
          <FormItem>
            <FormLabel>In-app update priority</FormLabel>
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
            <FormDescription>
              Apps using the Play in-app updates API can prompt harder for
              higher priorities.
            </FormDescription>
            <FormMessage />
          </FormItem>
        )}
      />
      <FormField
        control={form.control}
        name="upload_mapping"
        render={({ field }) => (
          <FormItem className="flex flex-row items-start gap-3 sm:col-span-2">
            <FormControl>
              <Switch
                checked={field.value}
                onCheckedChange={field.onChange}
                aria-label="Upload the R8 mapping file"
              />
            </FormControl>
            <div className="flex flex-col gap-1">
              <FormLabel>Upload mapping.txt</FormLabel>
              <FormDescription>
                Play Console deobfuscates crash reports with it. Skipped when
                the build produced none.
              </FormDescription>
            </div>
          </FormItem>
        )}
      />
    </div>
  )
}

function GooglePlayPublishForm({
  canWrite,
  pipelineId,
  projectId,
  saved,
  secretScopes,
}: {
  canWrite: boolean
  pipelineId: string
  projectId: string
  saved: GooglePlayPublishResponse
  secretScopes: Map<string, SecretScope>
}) {
  const updateMutation = useUpdatePipelineGooglePlayPublish(pipelineId)
  const [addSecretOpen, setAddSecretOpen] = useState(false)
  const form = useForm<FormValues>({
    resolver: zodResolver(formSchema),
    values: toFormValues(saved.config),
    resetOptions: { keepDirtyValues: true },
  })
  const { isDirty } = useFormState({ control: form.control })
  const isNew = !saved.config

  function onSubmit(next: FormValues) {
    updateMutation.mutate(toConfig(next), {
      onSuccess: (response) => {
        form.reset(toFormValues(response.config))
        toast.success('Google Play settings saved')
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

            <SettingsSection title="Credentials">
              <ServiceAccountField
                form={form}
                saved={saved}
                secretScopes={secretScopes}
                onAddSecret={() => setAddSecretOpen(true)}
              />
            </SettingsSection>

            <SettingsSurfaceSeparator />

            <SettingsSection title="Release">
              <ReleaseFields form={form} />
            </SettingsSection>

            <SettingsSurfaceSeparator />

            <SettingsSection title="Advanced">
              <AdvancedFields form={form} />
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
                    {isNew ? 'Turn on Google Play publishing' : 'Save changes'}
                  </Button>
                </div>
              </>
            ) : null}
          </SettingsSurface>
        </fieldset>
      </form>
      <SecretValueDialog
        open={addSecretOpen}
        onOpenChange={(open) => {
          setAddSecretOpen(open)
        }}
        defaultName="PLAY_SERVICE_ACCOUNT"
        target={{ scope: 'project', projectId }}
      />
    </Form>
  )
}

export function GooglePlayPublishSection({
  androidSigning,
  canReadInstanceSecrets,
  canWrite,
  pipelineId,
  projectId,
}: {
  androidSigning: PipelineAndroidSigningResponse | undefined
  canReadInstanceSecrets: boolean
  canWrite: boolean
  pipelineId: string
  projectId: string
}) {
  const publishQuery = usePipelineGooglePlayPublish(pipelineId)
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
  const releaseSigned =
    !!androidSigning?.release.enabled && androidSigning.release.has_keystore

  return (
    <SettingsSection
      title="Google Play"
      description="Upload the signed App Bundle from each successful build. Every Google Play call runs on the runner, so the service account key never leaves it."
    >
      {androidSigning && !releaseSigned ? (
        <Alert>
          <HugeiconsIcon icon={InformationCircleIcon} />
          <AlertDescription>
            Release signing is not set up for this pipeline. Google Play rejects
            bundles that are not signed with your upload key.
          </AlertDescription>
        </Alert>
      ) : null}

      {publishQuery.error ? (
        <CollectionError
          title="Failed to load Google Play settings"
          description={publishQuery.error.message}
          onRetry={() => void publishQuery.refetch()}
        />
      ) : publishQuery.data ? (
        <GooglePlayPublishForm
          canWrite={canWrite}
          pipelineId={pipelineId}
          projectId={projectId}
          saved={publishQuery.data}
          secretScopes={secretScopes}
        />
      ) : (
        <Skeleton className="h-96 w-full" />
      )}
    </SettingsSection>
  )
}
