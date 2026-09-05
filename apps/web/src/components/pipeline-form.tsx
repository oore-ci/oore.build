import type { IosSigningFiles } from '@/lib/pipeline-signing'
import { useState } from 'react'
import { useBlocker } from '@tanstack/react-router'
import { useForm, useWatch } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import { HugeiconsIcon } from '@hugeicons/react'
import { AlertCircleIcon } from '@hugeicons/core-free-icons'

import type { PipelineFormValues } from '@/lib/pipeline-schema'
import type {
  PipelineAndroidSigningResponse,
  PipelineIosSigningResponse,
} from '@oore/client/models'
import { Button } from '@/components/ui/button'
import { Form } from '@/components/ui/form'
import { Spinner } from '@/components/ui/spinner'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { pipelineFormSchema } from '@/lib/pipeline-schema'
import {
  parseEnvVars,
  hasSigningFileChanges,
  parseBundleIdsInput,
  parseMultiline,
  previewPlatformCommands,
  selectedPlatforms,
} from '@/lib/pipeline-form-utils'
import {
  PipelineIdentityAndConfigSection,
  PipelineTriggersSection,
} from '@/components/pipeline-form-basic-sections'
import {
  PipelineCommandsSection,
  PipelinePlatformArgsSection,
} from '@/components/pipeline-form-build-sections'
import {
  PipelineArtifactsSection,
  PipelineEnvironmentSection,
} from '@/components/pipeline-form-output-sections'
import { PipelineAndroidSigningSection } from '@/components/pipeline-form-android-signing-section'
import { PipelineIosSigningSection } from '@/components/pipeline-form-ios-signing-section'
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog'

interface PipelineFormProps {
  compactSetup?: boolean
  initialValues: PipelineFormValues
  onSubmit: (
    data: PipelineFormValues,
    releaseKeystoreFile: File | null,
    debugKeystoreFile: File | null,
    iosSigningFiles: IosSigningFiles,
  ) => Promise<void>
  onCancel: () => void
  submitLabel: string
  isPending: boolean
  validationErrors?: Array<string>
  /** Read-only repository workflow summary. When present, repository config owns execution fields. */
  repositoryWorkflow?: React.ReactNode
  /** Content rendered after all form sections but before the sticky action bar */
  children?: React.ReactNode
  /** Local-mode repositories only support manual/API build triggers for now. */
  manualOnlyTriggers?: boolean
  readOnly?: boolean
  readOnlyReason?: string
  retrySigning?: 'android' | 'ios'
  signingError?: string
  signingData?: PipelineAndroidSigningResponse
  iosSigningData?: PipelineIosSigningResponse
}

export default function PipelineForm({
  compactSetup = false,
  initialValues,
  onSubmit,
  onCancel,
  submitLabel,
  isPending,
  validationErrors = [],
  repositoryWorkflow,
  children,
  manualOnlyTriggers = false,
  readOnly = false,
  readOnlyReason,
  retrySigning,
  signingError,
  signingData,
  iosSigningData,
}: PipelineFormProps) {
  const form = useForm<PipelineFormValues>({
    resolver: zodResolver(pipelineFormSchema),
    defaultValues: initialValues,
    mode: 'onBlur',
    shouldUnregister: false,
  })

  const [releaseKeystoreFile, setReleaseKeystoreFile] = useState<File | null>(
    null,
  )
  const [debugKeystoreFile, setDebugKeystoreFile] = useState<File | null>(null)
  const [iosP12File, setIosP12File] = useState<File | null>(null)
  const [iosApiKeyFile, setIosApiKeyFile] = useState<File | null>(null)
  const [iosProfileFiles, setIosProfileFiles] = useState<
    Record<string, File | null>
  >({})
  const { isSubmitting } = form.formState
  const signingFilesDirty = hasSigningFileChanges(
    [releaseKeystoreFile, debugKeystoreFile, iosP12File, iosApiKeyFile],
    iosProfileFiles,
  )
  const isDirty = form.formState.isDirty || signingFilesDirty
  const blocker = useBlocker({
    shouldBlockFn: () => isDirty && !isSubmitting,
    enableBeforeUnload: () => isDirty && !isSubmitting,
    withResolver: true,
  })
  function handleProfileFileChange(bundleId: string, file: File | null) {
    setIosProfileFiles((previous) => ({
      ...previous,
      [bundleId]: file,
    }))
  }

  function handleFormSubmit(data: PipelineFormValues) {
    return onSubmit(data, releaseKeystoreFile, debugKeystoreFile, {
      p12File: iosP12File,
      apiKeyFile: iosApiKeyFile,
      profileFiles: iosProfileFiles,
    })
  }

  const values = useWatch({
    control: form.control,
    // SAFETY: computed value is guaranteed to be of type PipelineFormValues
    compute: (values) => values as PipelineFormValues,
  })

  const configMode = values.config_mode
  const previewDefaults = previewPlatformCommands(values)

  const envVarCount = parseEnvVars(values.env_vars).length
  const artifactPatterns = parseMultiline(values.artifact_patterns)
  const platforms = selectedPlatforms(values)
  const preBuildCount = parseMultiline(values.pre_build_commands).length
  const buildCmdCount = parseMultiline(values.build_commands).length
  const postBuildCount = parseMultiline(values.post_build_commands).length
  const totalCmdCount = preBuildCount + buildCmdCount + postBuildCount
  const iosBundleIds = parseBundleIdsInput(values.ios_signing_bundle_ids)

  return (
    <Form {...form}>
      <form
        onSubmit={form.handleSubmit(handleFormSubmit)}
        className="space-y-4 pb-8"
      >
        <PipelineIdentityAndConfigSection
          defaultOpen={!compactSetup}
          configMode={configMode}
          platforms={platforms}
          previewDefaults={previewDefaults}
          repositoryWorkflow={repositoryWorkflow}
        />
        <PipelineTriggersSection manualOnlyTriggers={manualOnlyTriggers} />
        <PipelineCommandsSection
          commandCount={totalCmdCount}
          hidden={!!repositoryWorkflow}
        />
        <PipelinePlatformArgsSection hidden={!!repositoryWorkflow} />
        <PipelineEnvironmentSection
          envVarCount={envVarCount}
          hidden={!!repositoryWorkflow}
        />
        <PipelineArtifactsSection
          artifactPatterns={artifactPatterns}
          hidden={!!repositoryWorkflow}
        />
        {values.platform_android ? (
          <PipelineAndroidSigningSection
            defaultOpen={
              retrySigning === 'android' ||
              initialValues.android_signing_release_enabled ||
              initialValues.android_signing_debug_enabled
            }
            debugKeystoreFile={debugKeystoreFile}
            onDebugKeystoreFileChange={setDebugKeystoreFile}
            onReleaseKeystoreFileChange={setReleaseKeystoreFile}
            releaseKeystoreFile={releaseKeystoreFile}
            signingData={signingData}
          />
        ) : null}
        {values.platform_ios ? (
          <PipelineIosSigningSection
            apiKeyFile={iosApiKeyFile}
            bundleIds={iosBundleIds}
            defaultOpen={
              retrySigning === 'ios' || initialValues.ios_signing_enabled
            }
            onApiKeyFileChange={setIosApiKeyFile}
            onP12FileChange={setIosP12File}
            onProfileFileChange={handleProfileFileChange}
            p12File={iosP12File}
            profileFiles={iosProfileFiles}
            signingData={iosSigningData}
          />
        ) : null}

        {validationErrors.length > 0 ? (
          <div>
            <Alert variant="destructive" role="alert">
              <HugeiconsIcon icon={AlertCircleIcon} size={16} />
              <AlertDescription>
                <ul className="list-disc space-y-1 pl-4">
                  {validationErrors.map((err) => (
                    <li key={err}>{err}</li>
                  ))}
                </ul>
              </AlertDescription>
            </Alert>
          </div>
        ) : null}

        {signingError ? (
          <Alert variant="destructive">
            <HugeiconsIcon icon={AlertCircleIcon} size={16} />
            <AlertDescription>
              Pipeline creation completed, but {retrySigning} signing failed:{' '}
              {signingError}. Fix the signing fields below and retry only
              signing.
            </AlertDescription>
          </Alert>
        ) : null}

        {values.ios_signing_enabled &&
          (values.ios_signing_mode === 'api' ||
            values.ios_signing_mode === 'hybrid') &&
          children}
        <div className="sticky bottom-0 z-30 rounded-md border bg-surface/95 shadow-xs backdrop-blur supports-backdrop-filter:bg-surface/80">
          <div className="flex items-center justify-end gap-3 p-3">
            <Button type="button" variant="outline" onClick={onCancel}>
              Cancel
            </Button>
            <Button
              type="submit"
              disabled={isPending || readOnly}
              title={readOnly ? readOnlyReason : undefined}
            >
              {isPending ? (
                <>
                  <Spinner className="size-4" />
                  Saving...
                </>
              ) : readOnly ? (
                'Demo is read-only'
              ) : (
                submitLabel
              )}
            </Button>
          </div>
        </div>
      </form>
      <AlertDialog
        open={blocker.status === 'blocked'}
        onOpenChange={(open) => {
          if (!open && blocker.status === 'blocked') blocker.reset()
        }}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>
              Discard unsaved pipeline changes?
            </AlertDialogTitle>
            <AlertDialogDescription>
              Form values and selected signing files will be lost if you leave
              this page.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel
              onClick={() => {
                if (blocker.status === 'blocked') blocker.reset()
              }}
            >
              Stay
            </AlertDialogCancel>
            <AlertDialogAction
              onClick={() => {
                if (blocker.status === 'blocked') blocker.proceed()
              }}
            >
              Discard changes
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </Form>
  )
}
