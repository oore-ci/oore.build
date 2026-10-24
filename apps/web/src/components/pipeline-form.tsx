import type { IosSigningFiles } from '@/lib/pipeline-signing'
import {
  useEffect,
  useRef,
  useState,
  type ReactNode,
  type RefObject,
} from 'react'
import { useBlocker } from '@tanstack/react-router'
import { useForm, useFormState, useWatch } from 'react-hook-form'
import type { FieldErrors } from 'react-hook-form'
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
import { FormActions } from '@/components/form-actions'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { pipelineFormSchema } from '@/lib/pipeline-schema'
import {
  parseEnvVars,
  hasSigningFileChanges,
  parseBundleIdsInput,
  parseMultiline,
  previewPlatformCommands,
} from '@/lib/pipeline-form-utils'
import { PipelineBasicsSection } from '@/components/pipeline-form-basic-sections'
import type { PipelineWorkflowSource } from '@/components/pipeline-form-basic-sections'
import { PipelineSectionIndex } from '@/components/pipeline-form-section-index'
import { PipelineBuildSection } from '@/components/pipeline-form-build-sections'
import { PipelineTriggersSection } from '@/components/pipeline-form-triggers-section'
import { PipelinePlanPanel } from '@/components/pipeline-plan-panel'
import { PipelineSigningSection } from '@/components/pipeline-form-signing-section'
import type { IosAppStorePublishing } from '@/components/pipeline-form-ios-signing-section'
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
  /** Content rendered after all form sections but before the action bar */
  children?: React.ReactNode
  /** Local-mode repositories only support manual/API build triggers for now. */
  manualOnlyTriggers?: boolean
  /** Whether the viewer may change who must approve builds (maintainers and instance admins). */
  canChangeApproval?: boolean
  readOnly?: boolean
  readOnlyReason?: string
  retrySigning?: 'android' | 'ios'
  signingError?: string
  signingData?: PipelineAndroidSigningResponse
  iosSigningData?: PipelineIosSigningResponse
  /** Rendered beside the iOS signing heading, such as the profile sync button. */
  iosSigningActions?: ReactNode
  iosAppStorePublishing?: IosAppStorePublishing
  /** Google Play upload is on for this pipeline (unknown on new pipelines). */
  googlePlayUpload?: boolean
  /** The pipeline file's v2 workflows, for the Workflow selector. */
  workflowSource?: PipelineWorkflowSource
}

/** Fields with an error; an array field with several bad rows counts once. */
function countFieldErrors(errors: FieldErrors) {
  return Object.keys(errors).filter((name) => name !== 'root').length
}

/**
 * Bring the first invalid field into view and focus it. `block: 'center'`
 * keeps it clear of the sticky action bar at the bottom of the form.
 */
function revealFirstInvalidField(formRef: RefObject<HTMLFormElement | null>) {
  requestAnimationFrame(() => {
    const field = formRef.current?.querySelector<HTMLElement>(
      '[aria-invalid="true"]',
    )
    if (!field) return
    field.scrollIntoView({ block: 'center' })
    field.focus({ preventScroll: true })
  })
}

/** Open the per-platform disclosure when the saved pipeline already uses it. */
function hasPlatformOverrides(values: PipelineFormValues) {
  return [
    values.android_build_args,
    values.ios_build_args,
    values.macos_build_args,
    values.android_command_override,
    values.ios_command_override,
    values.macos_command_override,
  ].some((value) => (value ?? '').trim().length > 0)
}

export default function PipelineForm({
  initialValues,
  onSubmit,
  onCancel,
  submitLabel,
  isPending,
  validationErrors = [],
  repositoryWorkflow,
  children,
  manualOnlyTriggers = false,
  canChangeApproval = false,
  readOnly = false,
  readOnlyReason,
  retrySigning,
  signingError,
  signingData,
  iosSigningData,
  iosSigningActions,
  iosAppStorePublishing,
  googlePlayUpload = false,
  workflowSource,
}: PipelineFormProps) {
  const form = useForm<PipelineFormValues>({
    resolver: zodResolver(pipelineFormSchema),
    defaultValues: initialValues,
    mode: 'onBlur',
    shouldUnregister: false,
    // Focus is moved by `revealFirstInvalidField` so the field lands above
    // the sticky action bar instead of under it.
    shouldFocusError: false,
  })
  const formRef = useRef<HTMLFormElement>(null)
  const { errors, isSubmitted } = useFormState({ control: form.control })
  const errorCount = isSubmitted ? countFieldErrors(errors) : 0

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
  const previewDefaults = previewPlatformCommands(values, {
    playBundle: googlePlayUpload,
  })
  const sections = [
    { id: 'basics', label: 'Basics' },
    ...(repositoryWorkflow ? [] : [{ id: 'build', label: 'Build' }]),
    { id: 'plan', label: 'Plan' },
    ...(values.platform_android || values.platform_ios
      ? [{ id: 'signing', label: 'Signing' }]
      : []),
    { id: 'triggers', label: 'Triggers' },
  ]

  // Deep links such as `edit#ios-signing` land on their section once the
  // form has rendered.
  useEffect(() => {
    const id = window.location.hash.slice(1)
    if (!id) return
    requestAnimationFrame(() => {
      document.getElementById(id)?.scrollIntoView({ block: 'start' })
    })
  }, [])

  const envVarCount = parseEnvVars(values.env_vars).length
  const preBuildCount = parseMultiline(values.pre_build_commands).length
  const buildCmdCount = parseMultiline(values.build_commands).length
  const postBuildCount = parseMultiline(values.post_build_commands).length
  const totalCmdCount = preBuildCount + buildCmdCount + postBuildCount
  const iosBundleIds = parseBundleIdsInput(values.ios_signing_bundle_ids)

  return (
    <Form {...form}>
      <form
        ref={formRef}
        noValidate
        onSubmit={(event) =>
          void form.handleSubmit(handleFormSubmit, () =>
            revealFirstInvalidField(formRef),
          )(event)
        }
        className="flex flex-col gap-8"
      >
        <PipelineSectionIndex sections={sections} />
        <PipelineBasicsSection
          configMode={configMode}
          repositoryWorkflow={repositoryWorkflow}
          workflowSource={workflowSource}
        />
        {repositoryWorkflow ? null : (
          <PipelineBuildSection
            advancedOpen={hasPlatformOverrides(initialValues)}
            commandCount={totalCmdCount}
            envVarCount={envVarCount}
            playBundle={googlePlayUpload}
            previewDefaults={previewDefaults}
          />
        )}
        <PipelinePlanPanel
          playBundle={googlePlayUpload}
          repositoryConfig={!!repositoryWorkflow}
          values={values}
        />
        <PipelineSigningSection
          androidSigningData={signingData}
          bundleIds={iosBundleIds}
          debugKeystoreFile={debugKeystoreFile}
          iosActions={iosSigningActions}
          iosApiKeyFile={iosApiKeyFile}
          iosAppStorePublishing={iosAppStorePublishing}
          iosP12File={iosP12File}
          iosProfileFiles={iosProfileFiles}
          iosSigningData={iosSigningData}
          onApiKeyFileChange={setIosApiKeyFile}
          onDebugKeystoreFileChange={setDebugKeystoreFile}
          onP12FileChange={setIosP12File}
          onProfileFileChange={handleProfileFileChange}
          onReleaseKeystoreFileChange={setReleaseKeystoreFile}
          releaseKeystoreFile={releaseKeystoreFile}
          values={values}
        />
        <PipelineTriggersSection
          canChangeApproval={canChangeApproval}
          manualOnlyTriggers={manualOnlyTriggers}
        />

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
              The pipeline was created, but {retrySigning} signing failed:{' '}
              {signingError}. Fix the signing fields above and retry only
              signing.
            </AlertDescription>
          </Alert>
        ) : null}

        {values.ios_signing_enabled &&
          (values.ios_signing_mode === 'api' ||
            values.ios_signing_mode === 'hybrid') &&
          children}

        <FormActions
          disabled={readOnly}
          disabledReason={readOnlyReason}
          isPending={isPending}
          onCancel={onCancel}
          sticky={isDirty}
          submitLabel={readOnly ? 'Demo is read-only' : submitLabel}
        >
          {errorCount > 0 ? (
            <Button
              type="button"
              variant="link"
              className="mr-auto px-1 text-destructive"
              onClick={() => revealFirstInvalidField(formRef)}
            >
              <HugeiconsIcon icon={AlertCircleIcon} data-icon="inline-start" />
              Fix {errorCount} {errorCount === 1 ? 'field' : 'fields'} to save
            </Button>
          ) : null}
        </FormActions>
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
              variant="destructive"
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
