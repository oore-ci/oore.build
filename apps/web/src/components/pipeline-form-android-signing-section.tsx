import { useFormContext } from 'react-hook-form'

import type { PipelineFormValues } from '@/lib/pipeline-schema'
import type {
  AndroidSigningProfile,
  PipelineAndroidSigningResponse,
} from '@oore/client/models'
import { PipelineFormSubsection } from '@/components/pipeline-form-section'
import SetupHint from '@/components/setup-hint'
import { Checkbox } from '@/components/ui/checkbox'
import {
  FormControl,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
} from '@/components/ui/form'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'

const ANDROID_GRADLE_SIGNING_SNIPPET = `android {
    signingConfigs {
        release {
            storeFile file(System.getenv("OORE_ANDROID_KEYSTORE_PATH"))
            storePassword System.getenv("OORE_ANDROID_KEYSTORE_PASSWORD")
            keyAlias System.getenv("OORE_ANDROID_KEY_ALIAS")
            keyPassword System.getenv("OORE_ANDROID_KEY_PASSWORD")
        }
    }
    buildTypes {
        release {
            signingConfig signingConfigs.release
        }
    }
}`

export function PipelineAndroidSigningSection({
  debugKeystoreFile,
  onDebugKeystoreFileChange,
  onReleaseKeystoreFileChange,
  releaseKeystoreFile,
  signingData,
}: {
  debugKeystoreFile: File | null
  onDebugKeystoreFileChange: (file: File | null) => void
  onReleaseKeystoreFileChange: (file: File | null) => void
  releaseKeystoreFile: File | null
  signingData?: PipelineAndroidSigningResponse
}) {
  const form = useFormContext<PipelineFormValues>()
  const releaseEnabled = form.watch('android_signing_release_enabled')
  const debugEnabled = form.watch('android_signing_debug_enabled')

  return (
    <PipelineFormSubsection
      title="Android"
      description={signingSummary(releaseEnabled, debugEnabled)}
    >
      <SigningToggle
        name="android_signing_release_enabled"
        label="Enable release signing"
        stored={signingData?.release}
      />
      {releaseEnabled ? (
        <SigningCredentials
          kind="release"
          file={releaseKeystoreFile}
          onFileChange={onReleaseKeystoreFileChange}
          stored={signingData?.release}
        />
      ) : null}
      <SigningToggle
        name="android_signing_debug_enabled"
        label="Enable debug signing"
        stored={signingData?.debug}
      />
      {debugEnabled ? (
        <SigningCredentials
          kind="debug"
          file={debugKeystoreFile}
          onFileChange={onDebugKeystoreFileChange}
          stored={signingData?.debug}
        />
      ) : null}
      {releaseEnabled || debugEnabled ? (
        <SetupHint
          title="Android project setup"
          code={ANDROID_GRADLE_SIGNING_SNIPPET}
          items={[
            'For standard Flutter release builds, upload the keystore here and Oore prepares the signing files for the runner.',
            'For custom Gradle flavors or signingConfigs, read the OORE_ANDROID_* environment variables shown below.',
            'Oore also writes OORE_ANDROID_KEY_PROPERTIES_PATH if your Gradle setup prefers a generated key.properties file.',
          ]}
        />
      ) : null}
    </PipelineFormSubsection>
  )
}

function SigningToggle({
  label,
  name,
  stored,
}: {
  label: string
  name: 'android_signing_release_enabled' | 'android_signing_debug_enabled'
  stored?: AndroidSigningProfile
}) {
  const form = useFormContext<PipelineFormValues>()
  return (
    <FormField
      control={form.control}
      name={name}
      render={({ field }) => (
        <FormItem>
          <label className="flex items-center gap-2 text-sm font-medium">
            <Checkbox
              checked={field.value}
              onCheckedChange={(checked) => field.onChange(!!checked)}
            />
            {label}
          </label>
          {stored ? (
            <p className="text-xs text-muted-foreground">
              {stored.has_keystore
                ? `Stored keystore: ${stored.keystore_filename ?? 'present'}`
                : `No stored ${name.includes('release') ? 'release' : 'debug'} keystore`}
            </p>
          ) : null}
        </FormItem>
      )}
    />
  )
}

function SigningCredentials({
  file,
  kind,
  onFileChange,
  stored,
}: {
  file: File | null
  kind: 'release' | 'debug'
  onFileChange: (file: File | null) => void
  stored?: AndroidSigningProfile
}) {
  const form = useFormContext<PipelineFormValues>()
  const title = kind === 'release' ? 'Release' : 'Debug'
  const fieldPrefix = `android_signing_${kind}` as const
  const aliasName = `${fieldPrefix}_key_alias` as const
  const storePasswordName = `${fieldPrefix}_store_password` as const
  const keyPasswordName = `${fieldPrefix}_key_password` as const

  return (
    <div className="grid gap-3 rounded-[min(var(--radius-2xl),16px)] bg-muted/40 p-4">
      <div className="space-y-2">
        <Label htmlFor={`android-${kind}-keystore`}>
          {title} keystore (.jks)
        </Label>
        <Input
          id={`android-${kind}-keystore`}
          type="file"
          accept=".jks,.keystore"
          onChange={(event) => onFileChange(event.target.files?.[0] ?? null)}
        />
        <p className="text-xs text-muted-foreground">
          {file
            ? `Selected: ${file.name}`
            : stored?.has_keystore
              ? 'Keep existing keystore or select a new file'
              : 'Select a JKS/keystore file'}
        </p>
      </div>
      <FormField
        control={form.control}
        name={aliasName}
        render={({ field }) => (
          <FormItem>
            <FormLabel>{title} key alias</FormLabel>
            <FormControl>
              <Input
                placeholder={kind === 'release' ? 'upload' : 'androiddebugkey'}
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
        name={storePasswordName}
        render={({ field }) => (
          <FormItem>
            <FormLabel>{title} store password</FormLabel>
            <FormControl>
              <Input
                type="password"
                autoComplete="current-password"
                placeholder={
                  stored?.has_store_password
                    ? 'Leave empty to keep existing password'
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
      <FormField
        control={form.control}
        name={keyPasswordName}
        render={({ field }) => (
          <FormItem>
            <FormLabel>{title} key password</FormLabel>
            <FormControl>
              <Input
                type="password"
                autoComplete="current-password"
                placeholder={
                  stored?.has_key_password
                    ? 'Leave empty to keep existing password'
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

function signingSummary(releaseEnabled: boolean, debugEnabled: boolean) {
  if (releaseEnabled && debugEnabled)
    return 'Release and debug builds are signed.'
  if (releaseEnabled) return 'Release builds are signed.'
  if (debugEnabled) return 'Debug builds are signed.'
  return 'Optional. Needed before you distribute release builds.'
}
