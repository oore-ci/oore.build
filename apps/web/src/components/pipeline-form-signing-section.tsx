import type { ReactNode } from 'react'
import { Link, useParams } from '@tanstack/react-router'
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
import { Button } from '@/components/ui/button'

/**
 * Signing: one section with a subsection per selected platform. Each picks
 * an identity from the app's signing library, the only place keystores and
 * certificates are uploaded. Nothing renders when the pipeline builds
 * neither Android nor iOS.
 */
export function PipelineSigningSection({
  androidSigningData,
  bundleIds,
  iosActions,
  iosApiKeyFile,
  iosAppStorePublishing,
  iosP12File,
  iosProfileFiles,
  iosSigningData,
  onApiKeyFileChange,
  onP12FileChange,
  onProfileFileChange,
  platformNotes,
  readOnlyPlatforms,
  summary,
  values,
}: {
  /** Shown under a platform's picker, such as workflows that sign otherwise. */
  platformNotes?: Partial<Record<'android' | 'ios', ReactNode>>
  /**
   * Shown in place of a platform's picker when something else decides its
   * identity for every build, such as the pipeline's file.
   */
  readOnlyPlatforms?: Partial<Record<'android' | 'ios', ReactNode>>
  summary?: string
  androidSigningData?: PipelineAndroidSigningResponse
  bundleIds: Array<string>
  iosActions?: ReactNode
  iosApiKeyFile: File | null
  iosAppStorePublishing?: IosAppStorePublishing
  iosP12File: File | null
  iosProfileFiles: Record<string, File | null>
  iosSigningData?: PipelineIosSigningResponse
  onApiKeyFileChange: (file: File | null) => void
  onP12FileChange: (file: File | null) => void
  onProfileFileChange: (bundleId: string, file: File | null) => void
  values: Pick<PipelineFormValues, 'platform_android' | 'platform_ios'>
}) {
  const { projectId = '' } = useParams({ strict: false })
  if (!values.platform_android && !values.platform_ios) return null
  return (
    <PipelineFormSection
      id="signing"
      summary={summary}
      title="Signing"
      hint="Keystores and certificates live in the app's signing library. Builds are signed on the runner."
      actions={
        <Button
          size="sm"
          variant="ghost"
          nativeButton={false}
          render={
            <Link
              to="/projects/$projectId"
              params={{ projectId }}
              search={{ tab: 'settings' }}
              hash="signing"
            />
          }
        >
          Signing library
        </Button>
      }
    >
      {values.platform_android
        ? (readOnlyPlatforms?.android ?? (
            <>
              <PipelineAndroidSigningSection
                projectId={projectId}
                signingData={androidSigningData}
              />
              {platformNotes?.android}
            </>
          ))
        : null}
      {values.platform_android && values.platform_ios ? (
        <SettingsSurfaceSeparator />
      ) : null}
      {values.platform_ios
        ? (readOnlyPlatforms?.ios ?? (
            <>
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
                projectId={projectId}
                signingData={iosSigningData}
              />
              {platformNotes?.ios}
            </>
          ))
        : null}
    </PipelineFormSection>
  )
}
