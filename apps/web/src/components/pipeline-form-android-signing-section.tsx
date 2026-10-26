import { useFormContext } from 'react-hook-form'

import type { PipelineFormValues } from '@/lib/pipeline-schema'
import type {
  AndroidSigningProfile,
  PipelineAndroidSigningResponse,
} from '@oore/client/models'
import { PipelineFormSubsection } from '@/components/pipeline-form-section'
import { FileField } from '@/components/file-field'
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
import { Text } from '@/components/layout/text'
import { textVariants } from '@/lib/recipes'

const ANDROID_GRADLE_DOCS_URL =
  'https://docs.oore.build/build/sign/android/gradle'

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
      id="android-signing"
      title="Android"
      meta={
        <span className={textVariants({ variant: 'caption' })}>
          {signingSummary(releaseEnabled, debugEnabled)}
        </span>
      }
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
          items={[
            <span key="gradle">
              No Gradle changes needed. For each <code>flutter build apk</code>{' '}
              or <code>flutter build appbundle</code>, Oore gives Gradle your
              keystore, and it takes precedence over the app&apos;s own signing
              config. Output from other commands, such as <code>./gradlew</code>
              , is not covered.
            </span>,
            <span key="check">
              The <code>android-sign</code> step then checks each APK or App
              Bundle and re-signs any that Gradle did not sign with your key.
            </span>,
            <span key="variant">
              Commands with <code>--debug</code> use the debug keystore, and all
              others use the release keystore. A variant with signing turned off
              keeps the Gradle signature. Build one variant per run.
            </span>,
            'The build step can read the keystore and its passwords while Gradle runs. Oore removes them when the step ends.',
            <span key="local-signing">
              A local <code>key.properties</code> setup can stay, as long as
              Gradle still configures when the file is missing in CI.{' '}
              <a
                href={ANDROID_GRADLE_DOCS_URL}
                target="_blank"
                rel="noopener noreferrer"
                className="underline underline-offset-4"
              >
                Learn more
              </a>
            </span>,
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
          {stored && !field.value ? (
            <Text variant="muted">
              {stored.has_keystore
                ? `Stored keystore: ${stored.keystore_filename ?? 'present'}`
                : `No stored ${name.includes('release') ? 'release' : 'debug'} keystore`}
            </Text>
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
      <FileField
        id={`android-${kind}-keystore`}
        label={`${title} keystore (.jks)`}
        accept=".jks,.keystore"
        file={file}
        onFileChange={onFileChange}
        storedName={
          stored?.has_keystore ? (stored.keystore_filename ?? true) : null
        }
        emptyLabel="No keystore stored yet"
      />
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
