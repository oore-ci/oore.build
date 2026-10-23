import { useEffect, type ReactNode } from 'react'
import { Link } from '@tanstack/react-router'
import { useFormContext } from 'react-hook-form'

import { usesOwnApiKey, type PipelineFormValues } from '@/lib/pipeline-schema'
import type {
  IosExportMethod,
  IosProvisioningProfileSummary,
  PipelineIosSigningResponse,
} from '@oore/client/models'
import { useAppStoreConnectKeys } from '@/hooks/use-app-store-connect-keys'
import { useHasPermissions } from '@/hooks/use-permissions'
import { PipelineFormSubsection } from '@/components/pipeline-form-section'
import { FileField } from '@/components/file-field'
import SetupHint from '@/components/setup-hint'
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
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Textarea } from '@/components/ui/textarea'

const EXPORT_METHODS = {
  ad_hoc: 'Ad hoc',
  app_store: 'App Store',
} satisfies Record<IosExportMethod, string>

const EXPORT_METHOD_EFFECTS = {
  ad_hoc:
    'Signed for the registered devices in your profiles. Testers install from Oore install links.',
  app_store:
    'Signed with an App Store distribution profile for TestFlight and App Store uploads. Oore install links do not work for these builds.',
} satisfies Record<IosExportMethod, string>

const SIGNING_MODES = {
  manual: 'Manual (.p12 + provisioning profiles)',
  api: 'API (App Store Connect automation)',
  hybrid: 'Hybrid (manual cert + API automation)',
} satisfies Record<string, string>

const SIGNING_MODE_SUMMARIES = {
  manual: 'Signed with an uploaded certificate and profiles.',
  api: 'Signing assets synced from App Store Connect.',
  hybrid: 'Uploaded certificate with profiles synced from App Store Connect.',
} satisfies Record<keyof typeof SIGNING_MODES, string>

/**
 * Whether the pipeline uploads App Store builds anywhere (the Publishing
 * tab), with a link there. Omitted while unknown, as on a new pipeline.
 */
export interface IosAppStorePublishing {
  configured: boolean
  link: ReactNode
}

export function PipelineIosSigningSection({
  actions,
  apiKeyFile,
  appStorePublishing,
  bundleIds,
  onApiKeyFileChange,
  onP12FileChange,
  onProfileFileChange,
  p12File,
  profileFiles,
  signingData,
}: {
  /** Rendered beside the heading, such as the profile sync button. */
  actions?: ReactNode
  apiKeyFile: File | null
  appStorePublishing?: IosAppStorePublishing
  bundleIds: Array<string>
  onApiKeyFileChange: (file: File | null) => void
  onP12FileChange: (file: File | null) => void
  onProfileFileChange: (bundleId: string, file: File | null) => void
  p12File: File | null
  profileFiles: Record<string, File | null>
  signingData?: PipelineIosSigningResponse
}) {
  const form = useFormContext<PipelineFormValues>()
  const enabled = form.watch('ios_signing_enabled')
  const mode = form.watch('ios_signing_mode')

  return (
    <PipelineFormSubsection
      id="ios-signing"
      title="iOS"
      hint="Required to install on physical iOS devices."
      meta={
        <span className="text-xs text-muted-foreground">
          {enabled ? SIGNING_MODE_SUMMARIES[mode] : 'Off'}
        </span>
      }
      actions={enabled ? actions : undefined}
    >
      <IosSigningToggle signingData={signingData} />
      {enabled ? (
        <>
          <IosSigningIdentityFields
            appStorePublishing={appStorePublishing}
            mode={mode}
            storedExportMethod={signingData?.export_method}
          />
          {mode === 'manual' || mode === 'hybrid' ? (
            <IosCertificateFields
              file={p12File}
              onFileChange={onP12FileChange}
              signingData={signingData}
            />
          ) : null}
          {mode === 'api' || mode === 'hybrid' ? (
            <IosApiKeyFields
              file={apiKeyFile}
              onFileChange={onApiKeyFileChange}
              signingData={signingData}
            />
          ) : null}
          {mode === 'manual' || mode === 'hybrid' ? (
            <IosProvisioningProfiles
              bundleIds={bundleIds}
              onFileChange={onProfileFileChange}
              profileFiles={profileFiles}
              profiles={signingData?.provisioning_profiles ?? []}
            />
          ) : null}
          <SetupHint
            title="iOS project setup"
            items={[
              'Manual mode uses your uploaded .p12 certificate and .mobileprovision files.',
              'API mode uses App Store Connect credentials to sync signing assets; hybrid mode combines API sync with a manually uploaded certificate.',
              'During a build, Oore installs profiles, creates a temporary keychain, and pins CODE_SIGN_IDENTITY when a signing identity is available.',
              'Keep bundle identifiers aligned with the Xcode targets you expect to sign.',
            ]}
          />
        </>
      ) : null}
    </PipelineFormSubsection>
  )
}

function IosSigningToggle({
  signingData,
}: {
  signingData?: PipelineIosSigningResponse
}) {
  const form = useFormContext<PipelineFormValues>()
  return (
    <FormField
      control={form.control}
      name="ios_signing_enabled"
      render={({ field }) => (
        <FormItem>
          <label className="flex items-center gap-2 text-sm font-medium">
            <Checkbox
              checked={field.value}
              onCheckedChange={(checked) => field.onChange(!!checked)}
            />
            Enable iOS signing
          </label>
          {signingData ? (
            <p className="text-sm text-muted-foreground">
              {storedSigningSummary(signingData)}
            </p>
          ) : null}
          <FormMessage />
        </FormItem>
      )}
    />
  )
}

const STORED_MODE_LABELS = {
  manual: 'Manual',
  api: 'API',
  hybrid: 'Hybrid',
} satisfies Record<keyof typeof SIGNING_MODES, string>

/** What the server has stored, as one sentence. */
function storedSigningSummary(signingData: PipelineIosSigningResponse) {
  const mode = signingData.mode
  const parts: Array<string> = []
  if (mode !== 'api') {
    parts.push(
      signingData.has_p12 ? 'certificate stored' : 'no certificate yet',
    )
  }
  if (mode !== 'manual') {
    parts.push(
      signingData.shared_api_key
        ? `uses the shared key ${signingData.shared_api_key.name}`
        : signingData.has_api_key
          ? 'own API key stored'
          : 'no API key yet',
    )
  }
  return `Saved setup: ${STORED_MODE_LABELS[mode]} signing, ${parts.join(', ')}.`
}

/**
 * Ad hoc and App Store profiles are not interchangeable, so the daemon
 * discards the stored ones when the distribution changes. Say what replaces
 * them: the save syncs new ones from App Store Connect in API and hybrid
 * mode, while manual mode needs uploads.
 */
function exportMethodChangeHint(mode: PipelineFormValues['ios_signing_mode']) {
  return mode === 'manual'
    ? ' Saving discards the stored provisioning profiles, so upload profiles of the new type below.'
    : ' Saving discards the stored provisioning profiles and syncs new ones from App Store Connect.'
}

function IosSigningIdentityFields({
  appStorePublishing,
  mode,
  storedExportMethod,
}: {
  appStorePublishing?: IosAppStorePublishing
  mode: PipelineFormValues['ios_signing_mode']
  storedExportMethod?: IosExportMethod
}) {
  const form = useFormContext<PipelineFormValues>()
  return (
    <>
      <FormField
        control={form.control}
        name="ios_signing_export_method"
        render={({ field }) => (
          <FormItem>
            <FormLabel>Distribution</FormLabel>
            <Select
              value={field.value}
              onValueChange={field.onChange}
              items={EXPORT_METHODS}
            >
              <FormControl>
                <SelectTrigger>
                  <SelectValue />
                </SelectTrigger>
              </FormControl>
              <SelectContent>
                {Object.entries(EXPORT_METHODS).map(([key, value]) => (
                  <SelectItem key={key} value={key}>
                    {value}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            <FormDescription>
              {EXPORT_METHOD_EFFECTS[field.value]}
              {storedExportMethod && storedExportMethod !== field.value
                ? exportMethodChangeHint(mode)
                : ''}
            </FormDescription>
            {field.value === 'ad_hoc' && appStorePublishing?.configured ? (
              <p className="text-sm text-warning">
                TestFlight and App Store uploads need App Store distribution.
                Uploads stay off until you switch. {appStorePublishing.link}
              </p>
            ) : null}
            {field.value === 'app_store' && appStorePublishing ? (
              <p className="text-sm text-muted-foreground">
                {appStorePublishing.configured ? (
                  <>
                    Signed builds upload as set up on the Publishing tab.{' '}
                    {appStorePublishing.link}
                  </>
                ) : (
                  <>
                    No TestFlight or App Store upload is set up for this
                    pipeline, so each build produces an IPA for you to upload
                    yourself. {appStorePublishing.link}
                  </>
                )}
              </p>
            ) : null}
            <FormMessage />
          </FormItem>
        )}
      />
      <FormField
        control={form.control}
        name="ios_signing_mode"
        render={({ field }) => (
          <FormItem>
            <FormLabel>Signing mode</FormLabel>
            <Select
              value={field.value}
              onValueChange={field.onChange}
              items={SIGNING_MODES}
            >
              <FormControl>
                <SelectTrigger>
                  <SelectValue />
                </SelectTrigger>
              </FormControl>
              <SelectContent>
                {Object.entries(SIGNING_MODES).map(([key, value]) => (
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
      <FormField
        control={form.control}
        name="ios_signing_team_id"
        render={({ field }) => (
          <FormItem>
            <FormLabel>Apple Team ID</FormLabel>
            <FormControl>
              <Input placeholder="TEAM1234" className="font-mono" {...field} />
            </FormControl>
            <FormMessage />
          </FormItem>
        )}
      />
      <FormField
        control={form.control}
        name="ios_signing_bundle_ids"
        render={({ field }) => (
          <FormItem>
            <FormLabel>Bundle identifiers</FormLabel>
            <FormControl>
              <Textarea
                placeholder={'com.example.app\ncom.example.app.share-extension'}
                className="font-mono"
                rows={3}
                {...field}
              />
            </FormControl>
            <FormDescription>
              Main bundle first, then optional extension bundle IDs (one per
              line or comma-separated).
            </FormDescription>
            <FormMessage />
          </FormItem>
        )}
      />
    </>
  )
}

function IosCertificateFields({
  file,
  onFileChange,
  signingData,
}: {
  file: File | null
  onFileChange: (file: File | null) => void
  signingData?: PipelineIosSigningResponse
}) {
  const form = useFormContext<PipelineFormValues>()
  return (
    <div className="grid gap-3 rounded-[min(var(--radius-2xl),16px)] bg-muted/40 p-4">
      <FileField
        id="ios-signing-certificate"
        label="Distribution certificate (.p12)"
        accept=".p12"
        file={file}
        onFileChange={onFileChange}
        storedName={
          signingData?.has_p12 ? (signingData.p12_filename ?? true) : null
        }
        emptyLabel="No certificate stored yet"
      />
      <FormField
        control={form.control}
        name="ios_signing_p12_password"
        render={({ field }) => (
          <FormItem>
            <FormLabel>P12 password</FormLabel>
            <FormControl>
              <Input
                type="password"
                placeholder={
                  signingData?.has_p12_password
                    ? 'Leave empty to keep stored password'
                    : ''
                }
                className="font-mono"
                {...field}
              />
            </FormControl>
            <FormMessage />
          </FormItem>
        )}
      />
    </div>
  )
}

const OWN_KEY = 'own'

function IosApiKeyFields({
  file,
  onFileChange,
  signingData,
}: {
  file: File | null
  onFileChange: (file: File | null) => void
  signingData?: PipelineIosSigningResponse
}) {
  const form = useFormContext<PipelineFormValues>()
  const keysQuery = useAppStoreConnectKeys()
  const [canManageKeys] = useHasPermissions(['instance_settings:write'])
  const keys = keysQuery.data ?? []
  const source = form.watch('ios_signing_api_key_source')

  // Until someone chooses, prefer a shared key when the instance has one.
  useEffect(() => {
    if (source || !keysQuery.isSuccess) return
    form.setValue(
      'ios_signing_api_key_source',
      keysQuery.data[0]?.id ?? OWN_KEY,
      { shouldDirty: false },
    )
  }, [form, keysQuery.data, keysQuery.isSuccess, source])

  const sourceItems: Record<string, string> = Object.fromEntries([
    ...keys.map((key) => [key.id, `${key.name} (${key.key_id})`]),
    [OWN_KEY, "This pipeline's own key"],
  ])
  const sharedKey = keys.find((key) => key.id === source)
  const manageLink = canManageKeys ? (
    <Link
      to="/settings/store-credentials"
      className="underline underline-offset-4"
    >
      Store credentials
    </Link>
  ) : (
    'Store credentials'
  )

  return (
    <div className="grid gap-3 rounded-[min(var(--radius-2xl),16px)] bg-muted/40 p-4">
      <FormField
        control={form.control}
        name="ios_signing_api_key_source"
        render={({ field }) => (
          <FormItem>
            <FormLabel>App Store Connect API key</FormLabel>
            <Select
              value={field.value || OWN_KEY}
              onValueChange={field.onChange}
              items={sourceItems}
            >
              <FormControl>
                <SelectTrigger>
                  <SelectValue />
                </SelectTrigger>
              </FormControl>
              <SelectContent>
                {Object.entries(sourceItems).map(([value, label]) => (
                  <SelectItem key={value} value={value}>
                    {label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            <FormDescription>
              {keysQuery.isError ? (
                <span className="text-destructive">
                  Could not load the shared keys in {manageLink}.{' '}
                  <Button
                    type="button"
                    variant="link"
                    size="xs"
                    className="h-auto px-0 align-baseline text-sm text-destructive underline"
                    onClick={() => void keysQuery.refetch()}
                  >
                    Retry
                  </Button>
                </span>
              ) : sharedKey ? (
                <>
                  Shared with every pipeline that picks it. Issuer{' '}
                  <span className="font-mono">{sharedKey.issuer_id}</span>.
                  Manage it in {manageLink}.
                </>
              ) : keys.length > 0 ? (
                <>
                  Overrides the shared keys in {manageLink} for this pipeline
                  only.
                </>
              ) : (
                <>
                  Apps in the same Apple team can share one key from{' '}
                  {manageLink} instead.
                </>
              )}
            </FormDescription>
            <FormMessage />
          </FormItem>
        )}
      />
      {usesOwnApiKey(source) ? (
        <>
          <FormField
            control={form.control}
            name="ios_signing_api_key_id"
            render={({ field }) => (
              <FormItem>
                <FormLabel>API key ID</FormLabel>
                <FormControl>
                  <Input
                    placeholder="ABC123XYZ"
                    className="font-mono"
                    {...field}
                  />
                </FormControl>
                <FormMessage />
              </FormItem>
            )}
          />
          <FormField
            control={form.control}
            name="ios_signing_api_issuer_id"
            render={({ field }) => (
              <FormItem>
                <FormLabel>API issuer ID (UUID)</FormLabel>
                <FormControl>
                  <Input
                    placeholder="00000000-0000-0000-0000-000000000000"
                    className="font-mono"
                    {...field}
                  />
                </FormControl>
                <FormMessage />
              </FormItem>
            )}
          />
          <FileField
            id="ios-signing-api-key"
            label="App Store Connect key (.p8)"
            accept=".p8,text/plain"
            file={file}
            onFileChange={onFileChange}
            storedName={
              signingData?.has_api_key ? (signingData.api_key_id ?? true) : null
            }
            emptyLabel="No key stored yet"
          />
        </>
      ) : null}
    </div>
  )
}

function IosProvisioningProfiles({
  bundleIds,
  onFileChange,
  profileFiles,
  profiles,
}: {
  bundleIds: Array<string>
  onFileChange: (bundleId: string, file: File | null) => void
  profileFiles: Record<string, File | null>
  profiles: Array<IosProvisioningProfileSummary>
}) {
  const profilesByBundle = new Map(
    profiles.map((profile) => [profile.bundle_id, profile]),
  )
  return (
    <div className="grid gap-3 rounded-[min(var(--radius-2xl),16px)] bg-muted/40 p-4">
      <p className="text-sm font-medium">Provisioning profiles by bundle ID</p>
      <div>
        {bundleIds.length === 0 ? (
          <p className="text-sm text-muted-foreground">
            Add at least one bundle ID to attach provisioning profiles.
          </p>
        ) : (
          <div className="space-y-3">
            {bundleIds.map((bundleId) => {
              const existing = profilesByBundle.get(bundleId)
              const selectedFile = profileFiles[bundleId]
              return (
                <FileField
                  key={bundleId}
                  id={`ios-profile-${bundleId}`}
                  label={bundleId}
                  labelClassName="font-mono"
                  accept=".mobileprovision"
                  file={selectedFile ?? null}
                  onFileChange={(next) => onFileChange(bundleId, next)}
                  storedName={
                    existing?.has_profile
                      ? (existing.profile_filename ??
                        existing.profile_name ??
                        true)
                      : null
                  }
                  emptyLabel="No profile stored yet"
                />
              )
            })}
          </div>
        )}
      </div>
    </div>
  )
}
