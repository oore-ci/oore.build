import { useFormContext } from 'react-hook-form'

import type { PipelineFormValues } from '@/lib/pipeline-schema'
import type {
  IosExportMethod,
  IosProvisioningProfileSummary,
  PipelineIosSigningResponse,
} from '@oore/client/models'
import { PipelineFormSectionHeader } from '@/components/pipeline-form-section-header'
import SetupHint from '@/components/setup-hint'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Checkbox } from '@/components/ui/checkbox'
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible'
import {
  FormControl,
  FormDescription,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
} from '@/components/ui/form'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
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

export function PipelineIosSigningSection({
  apiKeyFile,
  bundleIds,
  defaultOpen,
  onApiKeyFileChange,
  onP12FileChange,
  onProfileFileChange,
  p12File,
  profileFiles,
  signingData,
}: {
  apiKeyFile: File | null
  bundleIds: Array<string>
  defaultOpen: boolean
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
    <Collapsible defaultOpen={defaultOpen}>
      <Card>
        <CollapsibleTrigger className="w-full cursor-pointer">
          <CardHeader>
            <PipelineFormSectionHeader
              title="iOS Signing"
              summary={enabled ? `${mode} mode` : 'Not configured'}
            />
          </CardHeader>
        </CollapsibleTrigger>
        <CollapsibleContent>
          <CardContent className="space-y-4">
            <SetupHint
              title="iOS project setup"
              items={[
                'Manual mode uses your uploaded .p12 certificate and .mobileprovision files.',
                'API mode uses App Store Connect credentials to sync signing assets; hybrid mode combines API sync with a manually uploaded certificate.',
                'During a build, Oore installs profiles, creates a temporary keychain, and pins CODE_SIGN_IDENTITY when a signing identity is available.',
                'Keep bundle identifiers aligned with the Xcode targets you expect to sign.',
              ]}
            />
            <IosSigningToggle enabled={enabled} signingData={signingData} />
            {enabled ? (
              <>
                <IosSigningIdentityFields
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
              </>
            ) : null}
          </CardContent>
        </CollapsibleContent>
      </Card>
    </Collapsible>
  )
}

function IosSigningToggle({
  enabled,
  signingData,
}: {
  enabled: boolean
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
            <p className="text-xs text-muted-foreground">
              Stored: mode {signingData.mode}, p12{' '}
              {signingData.has_p12 ? 'present' : 'missing'}, API key{' '}
              {signingData.has_api_key ? 'present' : 'missing'}
            </p>
          ) : null}
          {!enabled ? (
            <p className="text-xs text-muted-foreground">
              Required for installing on physical iOS devices. You'll need a
              distribution certificate (.p12) and provisioning profiles.
            </p>
          ) : null}
          <FormMessage />
        </FormItem>
      )}
    />
  )
}

function IosSigningIdentityFields({
  storedExportMethod,
}: {
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
                ? ' Saving discards the stored provisioning profiles, so add or sync profiles of the new type.'
                : ''}
            </FormDescription>
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
            <p className="text-xs text-muted-foreground">
              Main bundle first, then optional extension bundle IDs (one per
              line or comma-separated).
            </p>
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
    <Card size="sm">
      <CardContent className="grid gap-3">
        <div className="space-y-2">
          <Label htmlFor="ios-signing-certificate">
            Distribution certificate (.p12)
          </Label>
          <Input
            id="ios-signing-certificate"
            type="file"
            accept=".p12"
            onChange={(event) => onFileChange(event.target.files?.[0] ?? null)}
          />
          <p className="text-xs text-muted-foreground">
            {file
              ? `Selected: ${file.name}`
              : signingData?.has_p12
                ? `Stored p12: ${signingData.p12_filename ?? 'present'}`
                : 'Select a .p12 certificate file'}
          </p>
        </div>
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
      </CardContent>
    </Card>
  )
}

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
  return (
    <Card size="sm">
      <CardContent className="grid gap-3">
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
        <div className="space-y-2">
          <Label htmlFor="ios-signing-api-key">
            App Store Connect key (.p8)
          </Label>
          <Input
            id="ios-signing-api-key"
            type="file"
            accept=".p8,text/plain"
            onChange={(event) => onFileChange(event.target.files?.[0] ?? null)}
          />
          <p className="text-xs text-muted-foreground">
            {file
              ? `Selected: ${file.name}`
              : signingData?.has_api_key
                ? `Stored key: ${signingData.api_key_id ?? 'present'}`
                : 'Upload App Store Connect private key file (.p8)'}
          </p>
        </div>
      </CardContent>
    </Card>
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
    <Card size="sm">
      <CardHeader>
        <CardTitle>Provisioning profiles by bundle ID</CardTitle>
      </CardHeader>
      <CardContent>
        {bundleIds.length === 0 ? (
          <p className="text-xs text-muted-foreground">
            Add at least one bundle ID to attach provisioning profiles.
          </p>
        ) : (
          <div className="space-y-3">
            {bundleIds.map((bundleId) => {
              const existing = profilesByBundle.get(bundleId)
              const selectedFile = profileFiles[bundleId]
              return (
                <div key={bundleId} className="space-y-2">
                  <Label
                    htmlFor={`ios-profile-${bundleId}`}
                    className="font-mono text-xs"
                  >
                    {bundleId}
                  </Label>
                  <Input
                    id={`ios-profile-${bundleId}`}
                    type="file"
                    accept=".mobileprovision"
                    onChange={(event) =>
                      onFileChange(bundleId, event.target.files?.[0] ?? null)
                    }
                  />
                  <p className="text-xs text-muted-foreground">
                    {selectedFile
                      ? `Selected: ${selectedFile.name}`
                      : existing?.has_profile
                        ? `Stored profile: ${existing.profile_filename ?? existing.profile_name ?? 'present'}`
                        : 'Upload .mobileprovision for this bundle ID'}
                  </p>
                </div>
              )
            })}
          </div>
        )}
      </CardContent>
    </Card>
  )
}
