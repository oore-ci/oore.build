import type { ReactNode } from 'react'
import type { PipelineFormValues } from '@/lib/pipeline-schema'
import type {
  PipelineAndroidSigningResponse,
  PipelineIosSigningResponse,
} from '@oore/client/models'
import { PipelineAndroidSigningSection } from '@/components/pipeline-form-android-signing-section'
import {
  PipelineIosSigningSection,
  type IosAppStorePublishing,
} from '@/components/pipeline-form-ios-signing-section'
import { PipelineFormSection } from '@/components/pipeline-form-section'
import { SettingsSurfaceSeparator } from '@/components/settings/settings-section'

/**
 * Signing: one section with a subsection per selected platform. Nothing
 * renders when the pipeline builds neither Android nor iOS.
 */
export function PipelineSigningSection({
  androidSigningData,
  bundleIds,
  debugKeystoreFile,
  iosActions,
  iosApiKeyFile,
  iosAppStorePublishing,
  iosP12File,
  iosProfileFiles,
  iosSigningData,
  onApiKeyFileChange,
  onDebugKeystoreFileChange,
  onP12FileChange,
  onProfileFileChange,
  onReleaseKeystoreFileChange,
  releaseKeystoreFile,
  values,
}: {
  androidSigningData?: PipelineAndroidSigningResponse
  bundleIds: Array<string>
  debugKeystoreFile: File | null
  iosActions?: ReactNode
  iosApiKeyFile: File | null
  iosAppStorePublishing?: IosAppStorePublishing
  iosP12File: File | null
  iosProfileFiles: Record<string, File | null>
  iosSigningData?: PipelineIosSigningResponse
  onApiKeyFileChange: (file: File | null) => void
  onDebugKeystoreFileChange: (file: File | null) => void
  onP12FileChange: (file: File | null) => void
  onProfileFileChange: (bundleId: string, file: File | null) => void
  onReleaseKeystoreFileChange: (file: File | null) => void
  releaseKeystoreFile: File | null
  values: Pick<PipelineFormValues, 'platform_android' | 'platform_ios'>
}) {
  if (!values.platform_android && !values.platform_ios) return null
  return (
    <PipelineFormSection
      id="signing"
      title="Signing"
      description="Credentials stay on the server. Builds are signed on the runner."
    >
      {values.platform_android ? (
        <PipelineAndroidSigningSection
          debugKeystoreFile={debugKeystoreFile}
          onDebugKeystoreFileChange={onDebugKeystoreFileChange}
          onReleaseKeystoreFileChange={onReleaseKeystoreFileChange}
          releaseKeystoreFile={releaseKeystoreFile}
          signingData={androidSigningData}
        />
      ) : null}
      {values.platform_android && values.platform_ios ? (
        <SettingsSurfaceSeparator />
      ) : null}
      {values.platform_ios ? (
        <PipelineIosSigningSection
          actions={iosActions}
          apiKeyFile={iosApiKeyFile}
          appStorePublishing={iosAppStorePublishing}
          bundleIds={bundleIds}
          onApiKeyFileChange={onApiKeyFileChange}
          onP12FileChange={onP12FileChange}
          onProfileFileChange={onProfileFileChange}
          p12File={iosP12File}
          profileFiles={iosProfileFiles}
          signingData={iosSigningData}
        />
      ) : null}
    </PipelineFormSection>
  )
}
