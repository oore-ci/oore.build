import { useFormContext } from 'react-hook-form'

import {
  PIPELINE_OWN_FILES,
  type PipelineFormValues,
} from '@/lib/pipeline-schema'
import type {
  AndroidSigningProfile,
  PipelineAndroidSigningResponse,
} from '@oore/client/models'
import { PipelineFormSubsection } from '@/components/pipeline-form-section'
import SetupHint from '@/components/setup-hint'
import { SigningIdentityPicker } from '@/components/signing/signing-identity-picker'
import {
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
} from '@/components/ui/form'
import { textVariants } from '@/lib/recipes'

const ANDROID_GRADLE_DOCS_URL =
  'https://docs.oore.build/build/sign/android/gradle'

export function PipelineAndroidSigningSection({
  projectId,
  signingData,
}: {
  projectId: string
  signingData?: PipelineAndroidSigningResponse
}) {
  const form = useFormContext<PipelineFormValues>()
  const releaseEnabled = !!form.watch('android_signing_release_identity_id')
  const debugEnabled = !!form.watch('android_signing_debug_identity_id')

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
      <BuildTypeIdentity
        buildType="release"
        projectId={projectId}
        stored={signingData?.release}
      />
      <BuildTypeIdentity
        buildType="debug"
        projectId={projectId}
        stored={signingData?.debug}
      />
      {releaseEnabled || debugEnabled ? (
        <SetupHint
          title="Android app setup"
          summary="No Gradle changes needed."
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
              others use the release keystore. A variant with no keystore keeps
              the Gradle signature. Build one variant per run.
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

/**
 * The keystore one build type signs with, picked from the app's signing
 * library. Files the pipeline still holds from before the library stay a
 * choice until another is picked.
 */
function BuildTypeIdentity({
  buildType,
  projectId,
  stored,
}: {
  buildType: 'release' | 'debug'
  projectId: string
  stored?: AndroidSigningProfile
}) {
  const form = useFormContext<PipelineFormValues>()
  const name = `android_signing_${buildType}_identity_id` as const
  const enabledName = `android_signing_${buildType}_enabled` as const
  const ownFiles =
    stored?.has_keystore && !stored.identity_id
      ? ([
          [
            PIPELINE_OWN_FILES,
            `${stored.keystore_filename ?? 'Keystore'} (stored on this pipeline)`,
          ],
        ] as const)
      : []
  return (
    <FormField
      control={form.control}
      name={name}
      render={({ field }) => (
        <FormItem>
          <FormLabel
            hint={
              buildType === 'release'
                ? 'Signs every build except those run with --debug.'
                : 'Signs builds run with --debug.'
            }
          >
            {buildType === 'release' ? 'Release builds' : 'Debug builds'}
          </FormLabel>
          <SigningIdentityPicker
            emptyLabel="Not signed"
            extra={ownFiles}
            kind="android_keystore"
            projectId={projectId}
            value={field.value ?? ''}
            onChange={(value) => {
              field.onChange(value)
              form.setValue(enabledName, value !== '', { shouldDirty: true })
            }}
          />
          <FormMessage />
        </FormItem>
      )}
    />
  )
}

function signingSummary(releaseEnabled: boolean, debugEnabled: boolean) {
  if (releaseEnabled && debugEnabled)
    return 'Release and debug builds are signed.'
  if (releaseEnabled) return 'Release builds are signed.'
  if (debugEnabled) return 'Debug builds are signed.'
  return 'Not signed'
}
