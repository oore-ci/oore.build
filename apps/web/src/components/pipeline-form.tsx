import { useReducer, useRef, useState } from 'react'
import { useBlocker } from '@tanstack/react-router'
import { useForm } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import { HugeiconsIcon } from '@hugeicons/react'
import { AlertCircleIcon } from '@hugeicons/core-free-icons'

import type { PipelineFormValues } from '@/lib/pipeline-schema'
import { Button } from '@/components/ui/button'
import { Form } from '@/components/ui/form'
import { Spinner } from '@/components/ui/spinner'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { pipelineFormSchema } from '@/lib/pipeline-schema'
import {
  parseEnvVars,
  hasSigningFileChanges,
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
  initialValues: PipelineFormValues
  initialEvents: Array<string>
  initialCancelPrevious: boolean
  onSubmit: (
    data: PipelineFormValues,
    events: Array<string>,
    cancelPrevious: boolean,
    releaseKeystoreFile: File | null,
    debugKeystoreFile: File | null,
    iosSigningFiles: {
      p12File: File | null
      apiKeyFile: File | null
      profileFiles: Record<string, File | null>
    },
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
  signingData?: {
    release: {
      has_keystore: boolean
      keystore_filename?: string
      has_store_password: boolean
      has_key_password: boolean
    }
    debug: {
      has_keystore: boolean
      keystore_filename?: string
      has_store_password: boolean
      has_key_password: boolean
    }
  }
  iosSigningData?: {
    enabled: boolean
    mode: 'manual' | 'api' | 'hybrid'
    team_id?: string
    bundle_ids: Array<string>
    has_p12: boolean
    p12_filename?: string
    has_p12_password: boolean
    has_api_key: boolean
    api_key_id?: string
    api_issuer_id?: string
    provisioning_profiles: Array<{
      bundle_id: string
      has_profile: boolean
      profile_filename?: string
      profile_uuid?: string
      profile_name?: string
      expires_at?: number
    }>
  }
}

function parseBundleIdsInput(raw?: string): Array<string> {
  if (!raw) return []
  const seen = new Set<string>()
  const values: Array<string> = []
  for (const part of raw.split(/[\n,]/g)) {
    const trimmed = part.trim()
    if (!trimmed) continue
    if (seen.has(trimmed)) continue
    seen.add(trimmed)
    values.push(trimmed)
  }
  return values
}

interface PipelineSections {
  config: boolean
  triggers: boolean
  commands: boolean
  platformArgs: boolean
  env: boolean
  artifacts: boolean
  iosSigning: boolean
  signing: boolean
}

interface PipelineAuxiliaryState {
  cancelPrevious: boolean
  isDirty: boolean
  selectedEvents: Array<string>
}

type PipelineAuxiliaryAction =
  | { type: 'toggle_event'; event: string }
  | { type: 'set_cancel_previous'; checked: boolean }

function pipelineAuxiliaryReducer(
  state: PipelineAuxiliaryState,
  action: PipelineAuxiliaryAction,
): PipelineAuxiliaryState {
  if (action.type === 'set_cancel_previous') {
    return { ...state, cancelPrevious: action.checked, isDirty: true }
  }

  return {
    ...state,
    isDirty: true,
    selectedEvents: state.selectedEvents.includes(action.event)
      ? state.selectedEvents.filter((entry) => entry !== action.event)
      : [...state.selectedEvents, action.event],
  }
}

type PipelineSectionsAction =
  | {
      type: 'set'
      section: keyof PipelineSections
      open: boolean
    }
  | {
      type: 'reveal'
      sections: Partial<Record<keyof PipelineSections, boolean>>
    }

function pipelineSectionsReducer(
  state: PipelineSections,
  action: PipelineSectionsAction,
): PipelineSections {
  if (action.type === 'set') {
    return { ...state, [action.section]: action.open }
  }

  const next = { ...state }
  for (const [section, shouldOpen] of Object.entries(action.sections)) {
    if (shouldOpen) next[section as keyof PipelineSections] = true
  }
  return next
}

function initialPipelineSections({
  initialValues,
  retrySigning,
}: Pick<
  PipelineFormProps,
  'initialValues' | 'retrySigning'
>): PipelineSections {
  return {
    config: true,
    triggers: true,
    commands: false,
    platformArgs: false,
    env: false,
    artifacts: false,
    iosSigning: !!initialValues.ios_signing_enabled || retrySigning === 'ios',
    signing:
      retrySigning === 'android' ||
      !!(
        initialValues.android_signing_release_enabled ||
        initialValues.android_signing_debug_enabled
      ),
  }
}

export default function PipelineForm({
  initialValues,
  initialEvents,
  initialCancelPrevious,
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

  const [auxiliary, dispatchAuxiliary] = useReducer(pipelineAuxiliaryReducer, {
    cancelPrevious: initialCancelPrevious,
    isDirty: false,
    selectedEvents: initialEvents,
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
  const [sections, dispatchSections] = useReducer(
    pipelineSectionsReducer,
    { initialValues, retrySigning },
    initialPipelineSections,
  )
  const isSubmittingRef = useRef(false)
  const signingFilesDirty = hasSigningFileChanges(
    [releaseKeystoreFile, debugKeystoreFile, iosP12File, iosApiKeyFile],
    iosProfileFiles,
  )
  const isDirty =
    form.formState.isDirty || auxiliary.isDirty || signingFilesDirty
  const blocker = useBlocker({
    shouldBlockFn: () => isDirty && !isSubmittingRef.current,
    enableBeforeUnload: () => isDirty && !isSubmittingRef.current,
    withResolver: true,
  })
  const setSectionOpen = (section: keyof typeof sections) => (open: boolean) =>
    dispatchSections({ type: 'set', section, open })

  function toggleEvent(event: string) {
    dispatchAuxiliary({ type: 'toggle_event', event })
  }

  function handleCancelPreviousChange(checked: boolean) {
    dispatchAuxiliary({ type: 'set_cancel_previous', checked })
  }

  function handleProfileFileChange(bundleId: string, file: File | null) {
    setIosProfileFiles((previous) => ({
      ...previous,
      [bundleId]: file,
    }))
  }

  async function handleFormSubmit(data: PipelineFormValues) {
    isSubmittingRef.current = true
    try {
      await onSubmit(
        data,
        auxiliary.selectedEvents,
        auxiliary.cancelPrevious,
        releaseKeystoreFile,
        debugKeystoreFile,
        {
          p12File: iosP12File,
          apiKeyFile: iosApiKeyFile,
          profileFiles: iosProfileFiles,
        },
      )
    } finally {
      isSubmittingRef.current = false
    }
  }

  const values = form.watch()
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
        className="space-y-4 pb-24"
      >
        <PipelineIdentityAndConfigSection
          configMode={configMode}
          onOpenChange={setSectionOpen('config')}
          open={sections.config}
          platforms={platforms}
          previewDefaults={previewDefaults}
          repositoryWorkflow={repositoryWorkflow}
        />
        <PipelineTriggersSection
          cancelPrevious={auxiliary.cancelPrevious}
          manualOnlyTriggers={manualOnlyTriggers}
          onCancelPreviousChange={handleCancelPreviousChange}
          onOpenChange={setSectionOpen('triggers')}
          onToggleEvent={toggleEvent}
          open={sections.triggers}
          selectedEvents={auxiliary.selectedEvents}
        />
        <PipelineCommandsSection
          commandCount={totalCmdCount}
          hidden={!!repositoryWorkflow}
          onOpenChange={setSectionOpen('commands')}
          open={sections.commands}
        />
        <PipelinePlatformArgsSection
          hidden={!!repositoryWorkflow}
          onOpenChange={setSectionOpen('platformArgs')}
          open={sections.platformArgs}
        />
        <PipelineEnvironmentSection
          envVarCount={envVarCount}
          hidden={!!repositoryWorkflow}
          onOpenChange={setSectionOpen('env')}
          open={sections.env}
        />
        <PipelineArtifactsSection
          artifactPatterns={artifactPatterns}
          hidden={!!repositoryWorkflow}
          onOpenChange={setSectionOpen('artifacts')}
          open={sections.artifacts}
        />
        {values.platform_android ? (
          <PipelineAndroidSigningSection
            debugKeystoreFile={debugKeystoreFile}
            onDebugKeystoreFileChange={setDebugKeystoreFile}
            onOpenChange={setSectionOpen('signing')}
            onReleaseKeystoreFileChange={setReleaseKeystoreFile}
            open={sections.signing}
            releaseKeystoreFile={releaseKeystoreFile}
            signingData={signingData}
          />
        ) : null}
        {values.platform_ios ? (
          <PipelineIosSigningSection
            apiKeyFile={iosApiKeyFile}
            bundleIds={iosBundleIds}
            onApiKeyFileChange={setIosApiKeyFile}
            onOpenChange={setSectionOpen('iosSigning')}
            onP12FileChange={setIosP12File}
            onProfileFileChange={handleProfileFileChange}
            open={sections.iosSigning}
            p12File={iosP12File}
            profileFiles={iosProfileFiles}
            signingData={iosSigningData}
          />
        ) : null}

        {validationErrors.length > 0 ? (
          <div>
            <Alert variant="destructive">
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
        {/* Sticky action bar */}
        <div className="sticky bottom-0 z-30 -mx-4 border-t bg-surface/95 backdrop-blur supports-backdrop-filter:bg-surface/60 sm:-mx-6">
          <div className="flex items-center justify-end gap-3 px-6 py-3">
            <Button type="button" variant="outline" onClick={onCancel}>
              Cancel
            </Button>
            <Button
              type="button"
              disabled={isPending || readOnly}
              title={readOnly ? readOnlyReason : undefined}
              onClick={() => {
                void form.handleSubmit(handleFormSubmit)()
              }}
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
