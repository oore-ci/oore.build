import { useRef, useState } from 'react'
import { createFileRoute, useNavigate } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import { AlertCircleIcon, RefreshIcon } from '@hugeicons/core-free-icons'

import type { RepositoryWorkflowPreview } from '@oore/client/models'
import type { PipelineFormValues } from '@/lib/pipeline-schema'
import { toast } from '@/lib/toast'
import { useFirstAppScope, useFirstAppStore } from '@/stores/first-app-store'
import { canChangeApprovalPolicy } from '@/hooks/use-permissions'
import { useAuthStore } from '@/stores/auth-store'
import {
  getActiveInstanceOrRedirect,
  requireAuthOrRedirect,
} from '@/lib/instance-context'
import { requireProjectPermissionOrRedirect } from '@/lib/project-route-guard'
import {
  useCreatePipeline,
  useRepositoryConfigFiles,
  useRepositoryWorkflows,
  useValidatePipeline,
} from '@/hooks/use-pipelines'
import { useProject, useProjectAppDetection } from '@/hooks/use-projects'
import { pipelineRequestFromForm } from '@/lib/pipeline-form-utils'
import { defaultValues, fileValues } from '@/lib/new-pipeline'
import PageLayout from '@/components/page-layout'
import PageHeader from '@/components/page-header'
import { PageMeta } from '@/lib/seo'
import { SettingsSection } from '@/components/settings/settings-section'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Spinner } from '@/components/ui/spinner'
import { Row } from '@/components/layout/row'
import { Stack } from '@/components/layout/stack'
import { Text } from '@/components/layout/text'
import {
  NO_FILE,
  NewPipelineSource,
  type SourceOption,
} from './-new-pipeline-source'
import { NewPipelineWorkflow } from './-new-pipeline-workflow'

export const Route = createFileRoute('/projects/$projectId/pipelines/new')({
  staticData: {
    breadcrumb: {
      title: 'New pipeline',
    },
  },
  beforeLoad: async ({ params, ...context }) => {
    const instance = getActiveInstanceOrRedirect()
    const token = requireAuthOrRedirect(instance.id, context)
    await requireProjectPermissionOrRedirect({
      action: 'write',
      instance,
      projectId: params.projectId,
      resource: 'pipelines',
      token,
    })
  },
  component: NewPipelinePage,
})

/**
 * New pipeline in three steps: pick a repository file (or none), confirm
 * the name, create. Everything else is set on the pipeline page after.
 */
function NewPipelinePage() {
  const scope = useFirstAppScope()
  const updateProgress = useFirstAppStore((state) => state.update)
  const { projectId } = Route.useParams()
  const navigate = useNavigate()
  const { data: projectData } = useProject(projectId)
  const project = projectData?.project
  const hasRepository = !!project?.repository_id
  const workflowsQuery = useRepositoryWorkflows(projectId, undefined, {
    enabled: hasRepository,
  })
  const detection = useProjectAppDetection(projectId, {
    enabled: hasRepository,
  })
  // Which discovered files pipelines already build from.
  const configFilesQuery = useRepositoryConfigFiles(projectId, {
    enabled: hasRepository,
  })
  const manualOnlyTriggers = project?.repository_provider === 'local_git'
  const instanceRole = useAuthStore((state) => state.user?.role)
  const canChangeApproval = canChangeApprovalPolicy(
    instanceRole,
    project?.current_user_role,
  )
  const createMutation = useCreatePipeline()
  const validateMutation = useValidatePipeline()
  const createdPipelineId = useRef<string | null>(null)
  const [errors, setErrors] = useState<Array<string>>([])
  const [picked, setPicked] = useState<string | null>(null)
  const [nameDraft, setNameDraft] = useState<string | null>(null)
  const [workflowDraft, setWorkflowDraft] = useState<string | null>(null)
  // A file found by typing its path; discovery lists only `.oore*` files.
  const [typedFile, setTypedFile] = useState<{
    file: RepositoryWorkflowPreview
    reference: string
  } | null>(null)

  const configFiles = new Map(
    (configFilesQuery.data?.files ?? []).map((file) => [file.path, file]),
  )
  const inUse = (path: string) =>
    (configFiles.get(path)?.pipelines.length ?? 0) > 0
  const discovered = workflowsQuery.data?.workflows ?? []
  const files =
    typedFile && !discovered.some((file) => file.path === typedFile.file.path)
      ? [...discovered, typedFile.file]
      : discovered
  // Files no pipeline uses come first, so the default pick is a new one.
  const validFiles = files
    .filter((file) => file.valid)
    .sort((a, b) => Number(inUse(a.path)) - Number(inUse(b.path)))
  const invalidFiles = files.filter((file) => !file.valid)

  const noFile = defaultValues(detection.data?.platforms)
  const options: Array<SourceOption> = [
    ...validFiles.map((file) => ({
      key: file.path,
      file,
      inUse: inUse(file.path),
      values: fileValues(file, configFiles.get(file.path)?.suggested_name),
    })),
    { key: NO_FILE, values: noFile },
  ]
  const selected = options.find((option) => option.key === picked) ?? options[0]
  const workflows = selected.file?.workflows ?? []
  const workflow = workflowDraft ?? selected.values.workflow ?? ''
  const name = nameDraft ?? selected.values.name
  const values: PipelineFormValues = {
    ...selected.values,
    name,
    workflow,
    trigger_events: manualOnlyTriggers ? [] : selected.values.trigger_events,
  }
  const reference =
    typedFile && typedFile.file.path === selected.file?.path
      ? typedFile.reference
      : (workflowsQuery.data?.reference ?? project?.default_branch ?? 'main')
  const loading = hasRepository && workflowsQuery.isLoading
  const pending = createMutation.isPending || validateMutation.isPending

  function pick(key: string) {
    setPicked(key)
    setNameDraft(null)
    setWorkflowDraft(null)
    setErrors([])
  }

  async function create() {
    if (createdPipelineId.current) return
    if (!values.name.trim()) {
      setErrors(['Name the pipeline'])
      return
    }
    const payload = pipelineRequestFromForm(values, {
      canChangeApproval,
      manualOnlyTriggers,
    })
    try {
      const result = await validateMutation.mutateAsync(payload)
      if (!result.valid && result.errors?.length) {
        setErrors(result.errors)
        return
      }
    } catch {
      // Validation is best-effort; create reports real problems.
    }
    setErrors([])
    try {
      const created = await createMutation.mutateAsync({
        projectId,
        data: payload,
      })
      createdPipelineId.current = created.pipeline.id
      updateProgress(scope, { projectId })
      toast.success('Pipeline created')
      await navigate({
        to: '/projects/$projectId',
        params: { projectId },
        search: { run: '1', runPipeline: created.pipeline.id },
      })
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Unknown error'
      toast.error(`Failed to create pipeline: ${message}`)
    }
  }

  return (
    <PageLayout>
      <PageMeta title="New pipeline" />
      <PageHeader title="New pipeline" />
      <form
        className="flex flex-col gap-8"
        onSubmit={(event) => {
          event.preventDefault()
          void create()
        }}
      >
        <SettingsSection
          title="Build from"
          hint={
            hasRepository
              ? `Files read from ${reference}. Builds read the file from the commit being built.`
              : undefined
          }
        >
          {loading ? (
            <Row gap={3}>
              <Spinner className="size-4" />
              <Text variant="muted">
                Looking for Oore files on {project?.default_branch ?? 'main'}…
              </Text>
            </Row>
          ) : (
            <Stack gap={3}>
              {workflowsQuery.error ? (
                <Alert variant="destructive">
                  <HugeiconsIcon icon={AlertCircleIcon} size={16} />
                  <AlertDescription className="flex flex-wrap items-center justify-between gap-3">
                    <span>
                      Could not read the repository.{' '}
                      {workflowsQuery.error.message}
                    </span>
                    <Button
                      type="button"
                      size="sm"
                      variant="outline"
                      onClick={() => void workflowsQuery.refetch()}
                    >
                      <HugeiconsIcon icon={RefreshIcon} />
                      Retry
                    </Button>
                  </AlertDescription>
                </Alert>
              ) : null}
              <NewPipelineSource
                options={options}
                invalidFiles={invalidFiles}
                selected={selected}
                onPick={pick}
                lookup={
                  hasRepository
                    ? {
                        projectId,
                        onFound: (file, foundAt) => {
                          setTypedFile({ file, reference: foundAt })
                          pick(file.path)
                        },
                      }
                    : undefined
                }
              />
            </Stack>
          )}
        </SettingsSection>

        {selected.file && workflows.length > 1 ? (
          <NewPipelineWorkflow
            path={selected.file.path}
            workflows={workflows}
            value={workflow}
            onChange={setWorkflowDraft}
          />
        ) : null}

        <SettingsSection title="Name">
          <Stack gap={2}>
            <Label htmlFor="new-pipeline-name" className="sr-only">
              Pipeline name
            </Label>
            <Input
              id="new-pipeline-name"
              value={name}
              onChange={(event) => setNameDraft(event.target.value)}
              aria-invalid={errors.length > 0 && !name.trim()}
              autoComplete="off"
            />
          </Stack>
        </SettingsSection>

        {errors.length > 0 ? (
          <Alert variant="destructive">
            <HugeiconsIcon icon={AlertCircleIcon} size={16} />
            <AlertDescription>
              <ul className="list-disc space-y-1 pl-4">
                {errors.map((message) => (
                  <li key={message}>{message}</li>
                ))}
              </ul>
            </AlertDescription>
          </Alert>
        ) : null}

        <Row gap={2} className="flex-wrap">
          <Button type="submit" disabled={loading || pending}>
            {pending ? <Spinner className="size-4" /> : null}
            Create pipeline
          </Button>
          <Button
            type="button"
            variant="ghost"
            onClick={() =>
              void navigate({
                to: '/projects/$projectId',
                params: { projectId },
              })
            }
          >
            Cancel
          </Button>
        </Row>
      </form>
    </PageLayout>
  )
}
