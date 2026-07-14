import { useFormContext } from 'react-hook-form'

import type { PipelineFormValues } from '@/lib/pipeline-schema'
import { PipelineFormSectionHeader } from '@/components/pipeline-form-section-header'
import SetupHint from '@/components/setup-hint'
import { Card, CardContent, CardHeader } from '@/components/ui/card'
import { Checkbox } from '@/components/ui/checkbox'
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible'
import {
  FormControl,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
} from '@/components/ui/form'
import { Input } from '@/components/ui/input'
import { Separator } from '@/components/ui/separator'

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

interface StoredSigningFile {
  has_keystore: boolean
  keystore_filename?: string
  has_store_password: boolean
  has_key_password: boolean
}

interface AndroidSigningData {
  release: StoredSigningFile
  debug: StoredSigningFile
}

export function PipelineAndroidSigningSection({
  debugKeystoreFile,
  onDebugKeystoreFileChange,
  onOpenChange,
  onReleaseKeystoreFileChange,
  open,
  releaseKeystoreFile,
  signingData,
}: {
  debugKeystoreFile: File | null
  onDebugKeystoreFileChange: (file: File | null) => void
  onOpenChange: (open: boolean) => void
  onReleaseKeystoreFileChange: (file: File | null) => void
  open: boolean
  releaseKeystoreFile: File | null
  signingData?: AndroidSigningData
}) {
  const form = useFormContext<PipelineFormValues>()
  const releaseEnabled = form.watch('android_signing_release_enabled')
  const debugEnabled = form.watch('android_signing_debug_enabled')

  return (
    <Collapsible open={open} onOpenChange={onOpenChange}>
      <Card>
        <CollapsibleTrigger className="w-full cursor-pointer">
          <CardHeader>
            <PipelineFormSectionHeader
              title="Android Signing"
              summary={signingSummary(releaseEnabled, debugEnabled)}
              open={open}
            />
          </CardHeader>
        </CollapsibleTrigger>
        <CollapsibleContent>
          <CardContent className="space-y-4">
            <SetupHint
              title="Android project setup"
              code={ANDROID_GRADLE_SIGNING_SNIPPET}
              items={[
                'For standard Flutter release builds, upload the keystore here and Oore prepares the signing files for the runner.',
                'For custom Gradle flavors or signingConfigs, read the OORE_ANDROID_* environment variables shown below.',
                'Oore also writes OORE_ANDROID_KEY_PROPERTIES_PATH if your Gradle setup prefers a generated key.properties file.',
              ]}
            />
            <SigningToggle
              name="android_signing_release_enabled"
              label="Enable release signing"
              stored={signingData?.release}
              showOptionalHint={!releaseEnabled && !debugEnabled}
            />
            {releaseEnabled ? (
              <SigningCredentials
                kind="release"
                file={releaseKeystoreFile}
                onFileChange={onReleaseKeystoreFileChange}
                stored={signingData?.release}
              />
            ) : null}
            <Separator />
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
          </CardContent>
        </CollapsibleContent>
      </Card>
    </Collapsible>
  )
}

function SigningToggle({
  label,
  name,
  showOptionalHint = false,
  stored,
}: {
  label: string
  name: 'android_signing_release_enabled' | 'android_signing_debug_enabled'
  showOptionalHint?: boolean
  stored?: StoredSigningFile
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
          {showOptionalHint ? (
            <p className="text-xs text-muted-foreground">
              Signing is optional for debug builds. Enable it when you're ready
              to distribute release builds.
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
  stored?: StoredSigningFile
}) {
  const form = useFormContext<PipelineFormValues>()
  const title = kind === 'release' ? 'Release' : 'Debug'
  const fieldPrefix = `android_signing_${kind}` as const
  const aliasName = `${fieldPrefix}_key_alias` as const
  const storePasswordName = `${fieldPrefix}_store_password` as const
  const keyPasswordName = `${fieldPrefix}_key_password` as const

  return (
    <div className="grid gap-3 border p-3">
      <FormItem>
        <FormLabel>{title} keystore (.jks)</FormLabel>
        <FormControl>
          <Input
            type="file"
            accept=".jks,.keystore"
            onChange={(event) => onFileChange(event.target.files?.[0] ?? null)}
          />
        </FormControl>
        <p className="text-xs text-muted-foreground">
          {file
            ? `Selected: ${file.name}`
            : stored?.has_keystore
              ? 'Keep existing keystore or select a new file'
              : 'Select a JKS/keystore file'}
        </p>
      </FormItem>
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
  if (!releaseEnabled && !debugEnabled) return 'Not configured'
  return (
    [releaseEnabled ? 'release' : '', debugEnabled ? 'debug' : '']
      .filter(Boolean)
      .join(' + ') + ' enabled'
  )
}
