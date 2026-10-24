import { useState } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  AlertCircleIcon,
  InformationCircleIcon,
} from '@hugeicons/core-free-icons'

import type {
  PipelinePlanStep,
  PlanPipelineRequest,
  StepPhase,
} from '@oore/client/models'
import type { PipelineFormValues } from '@/lib/pipeline-schema'
import { useDebouncedJson } from '@/hooks/use-debounced-json'
import { usePipelinePlan } from '@/hooks/use-pipelines'
import { selectedPlatforms } from '@/lib/pipeline-form-utils'
import type {
  PipelineConfigSource,
  PipelinePublishSettings,
} from '@/lib/pipeline-yaml-export'
import {
  pipelinesReadingPath,
  planConfigFromForm,
  repositoryFilePath,
  yamlExportRequest,
} from '@/lib/pipeline-yaml-export'
import { ALL_PLATFORMS, effectivePlanSelection } from '@/lib/pipeline-plan'
import { BUILD_PLATFORM_LABELS } from '@/lib/build-platforms'
import { getApiErrorMessage } from '@/lib/api-client/api-error'
import { textVariants } from '@/lib/recipes'
import { PipelineFormSection } from '@/components/pipeline-form-section'
import { PipelineRepositoryFile } from '@/components/pipeline-repository-file'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Label } from '@/components/ui/label'
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Skeleton } from '@/components/ui/skeleton'
import { Spinner } from '@/components/ui/spinner'
import { Textarea } from '@/components/ui/textarea'
import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group'
import { Mono } from '@/components/layout/mono'
import { Row } from '@/components/layout/row'
import { Stack } from '@/components/layout/stack'
import { Text } from '@/components/layout/text'

/** Edits settle for this long before the plan is requested again. */
const PLAN_DEBOUNCE_MS = 400

const PHASE_LABELS = {
  pre_build: 'Before build',
  build: 'Build',
  post_build: 'After build',
} satisfies Record<StepPhase, string>

const PHASES: Array<StepPhase> = ['pre_build', 'build', 'post_build']

type PlanSource = 'form' | 'file'

/**
 * "What will run": the steps a build resolves to, from the daemon's own
 * validator and planner, for the form's settings or a pasted `.oore.yaml`.
 */
export function PipelinePlanPanel({
  manualOnlyTriggers = false,
  otherPipelines = [],
  playBundle,
  publish,
  repositoryConfig,
  values,
}: {
  /** The repository only builds manually, so the exported file has no `on`. */
  manualOnlyTriggers?: boolean
  /** The project's other pipelines, to warn when they read the same file. */
  otherPipelines?: ReadonlyArray<PipelineConfigSource>
  /** Google Play upload is on, so the default Android build adds a bundle. */
  playBundle: boolean
  /** The pipeline's Publish settings, for the exported file's `publish`. */
  publish?: PipelinePublishSettings
  /** The repository's `.oore.yaml` owns execution, so start on the file. */
  repositoryConfig: boolean
  values: PipelineFormValues
}) {
  const [source, setSource] = useState<PlanSource>(
    repositoryConfig ? 'file' : 'form',
  )
  const [yaml, setYaml] = useState('')
  const [workflow, setWorkflow] = useState<string | null>(null)
  const [platform, setPlatform] = useState<string>(ALL_PLATFORMS)

  const [body, setBody] = useState<PlanPipelineRequest | null>(null)
  const planQuery = usePipelinePlan(body)
  const result = body ? planQuery.data : undefined

  // The last plan says which workflows and platforms exist, so a choice that
  // is no longer offered falls back instead of locking the panel on an error
  // with its select hidden. The form's own platforms are known up front.
  const workflows = result?.workflows ?? []
  const platforms =
    source === 'form'
      ? selectedPlatforms(values)
      : (result?.available_platforms ?? [])
  const selection = effectivePlanSelection({
    platform,
    platforms,
    // Until one is picked here, plan the workflow the pipeline builds.
    workflow: workflow ?? (values.workflow || null),
    workflows,
  })

  const request: PlanPipelineRequest | null =
    source === 'form'
      ? { execution_config: planConfigFromForm(values) }
      : yaml.trim()
        ? { repository_yaml: yaml, workflow: selection.workflow }
        : null
  if (request) {
    request.build_play_bundle = playBundle
    if (selection.platform) request.platforms = [selection.platform]
  }
  useDebouncedJson(request, PLAN_DEBOUNCE_MS, setBody)

  // Form settings show the file they would make next to their plan, unless
  // the repository's own file already owns execution.
  const showExport = source === 'form' && !repositoryConfig
  const filePath = repositoryFilePath(values)

  function changeSource(next: PlanSource) {
    setSource(next)
    setWorkflow(null)
    setPlatform(ALL_PLATFORMS)
  }

  return (
    <PipelineFormSection
      id="plan"
      title="What will run"
      hint="Steps come from the same planner the runner uses; secrets stay as references."
      actions={
        planQuery.isFetching ? <Spinner aria-label="Updating plan" /> : null
      }
    >
      <Stack gap={4}>
        <ToggleGroup
          aria-label="Plan from"
          value={[source]}
          variant="outline"
          spacing={0}
          onValueChange={(value) => {
            const next = value.at(0)
            if (next === 'form' || next === 'file') changeSource(next)
          }}
        >
          <ToggleGroupItem value="form">Form settings</ToggleGroupItem>
          <ToggleGroupItem value="file">.oore.yaml</ToggleGroupItem>
        </ToggleGroup>

        <div
          className={
            source === 'file' || showExport
              ? 'grid items-start gap-4 lg:grid-cols-2'
              : ''
          }
        >
          {showExport ? (
            <div className="min-w-0 lg:sticky lg:top-24">
              <PipelineRepositoryFile
                manualOnlyTriggers={manualOnlyTriggers}
                path={filePath}
                request={yamlExportRequest(values, {
                  manualOnlyTriggers,
                  publish,
                })}
                sharedWith={pipelinesReadingPath(filePath, otherPipelines)}
                values={values}
              />
            </div>
          ) : null}
          {source === 'file' ? (
            <Stack gap={2} className="lg:sticky lg:top-24">
              <Label htmlFor="plan-yaml">Repository file</Label>
              <Textarea
                id="plan-yaml"
                value={yaml}
                onChange={(event) => setYaml(event.target.value)}
                placeholder="Paste a .oore.yaml (version 1 or 2) to see what it runs"
                spellCheck={false}
                rows={12}
                className="max-h-[36rem] font-mono text-xs"
              />
            </Stack>
          ) : null}
          <Stack gap={4} className="min-w-0">
            {workflows.length > 1 || platforms.length > 1 ? (
              <div className="grid gap-3 sm:grid-cols-2">
                {workflows.length > 1 ? (
                  <Stack gap={2}>
                    <Label>Workflow</Label>
                    <Select
                      value={selection.workflow ?? workflows[0]}
                      onValueChange={(value) => setWorkflow(value)}
                    >
                      <SelectTrigger className="w-full" aria-label="Workflow">
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        <SelectGroup>
                          {workflows.map((name) => (
                            <SelectItem key={name} value={name}>
                              {name}
                            </SelectItem>
                          ))}
                        </SelectGroup>
                      </SelectContent>
                    </Select>
                  </Stack>
                ) : null}
                {platforms.length > 1 ? (
                  <Stack gap={2}>
                    <Label>Platform</Label>
                    <Select
                      value={selection.platform ?? ALL_PLATFORMS}
                      onValueChange={(value) =>
                        setPlatform(value ?? ALL_PLATFORMS)
                      }
                      items={{
                        [ALL_PLATFORMS]: 'All platforms',
                        ...BUILD_PLATFORM_LABELS,
                      }}
                    >
                      <SelectTrigger className="w-full" aria-label="Platform">
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        <SelectGroup>
                          <SelectItem value={ALL_PLATFORMS}>
                            All platforms
                          </SelectItem>
                          {platforms.map((key) => (
                            <SelectItem key={key} value={key}>
                              {BUILD_PLATFORM_LABELS[key]}
                            </SelectItem>
                          ))}
                        </SelectGroup>
                      </SelectContent>
                    </Select>
                  </Stack>
                ) : null}
              </div>
            ) : null}

            <PlanResult
              empty={!request}
              error={planQuery.isError ? planQuery.error : null}
              loading={!!body && planQuery.isPending}
              result={result}
            />
          </Stack>
        </div>
      </Stack>
    </PipelineFormSection>
  )
}

function PlanResult({
  empty,
  error,
  loading,
  result,
}: {
  empty: boolean
  error: Error | null
  loading: boolean
  result: ReturnType<typeof usePipelinePlan>['data']
}) {
  if (empty) {
    return (
      <Text>
        Paste the file to check it with the same rules as{' '}
        <Mono>oore pipeline validate</Mono>.
      </Text>
    )
  }
  if (error) {
    return (
      <Alert variant="destructive">
        <HugeiconsIcon icon={AlertCircleIcon} size={16} />
        <AlertDescription>
          Could not plan this configuration: {getApiErrorMessage(error, {})}
        </AlertDescription>
      </Alert>
    )
  }
  if (loading || !result) {
    return (
      <Stack gap={2} aria-busy="true">
        <Skeleton className="h-4 w-32" />
        <Skeleton className="h-14 w-full" />
        <Skeleton className="h-14 w-full" />
      </Stack>
    )
  }

  const plan = result.plan
  return (
    <Stack gap={4}>
      {result.errors.length > 0 ? (
        <Alert variant="destructive">
          <HugeiconsIcon icon={AlertCircleIcon} size={16} />
          <AlertDescription>
            <ul className="list-disc space-y-1 pl-4">
              {result.errors.map((message, index) => (
                <li key={index}>{message}</li>
              ))}
            </ul>
          </AlertDescription>
        </Alert>
      ) : null}
      {result.notices.length > 0 ? (
        <Alert>
          <HugeiconsIcon icon={InformationCircleIcon} size={16} />
          <AlertDescription>
            <ul className="list-disc space-y-1 pl-4">
              {result.notices.map((message, index) => (
                <li key={index}>{message}</li>
              ))}
            </ul>
          </AlertDescription>
        </Alert>
      ) : null}
      {plan ? (
        <>
          {PHASES.map((phase) => {
            const steps = plan.steps.filter((step) => step.phase === phase)
            if (steps.length === 0) return null
            return (
              <Stack key={phase} gap={2} render={<section />}>
                <Row gap={2} render={<h4 />} className="text-sm font-medium">
                  {PHASE_LABELS[phase]}
                  <span className={textVariants({ variant: 'caption' })}>
                    {steps.length}
                  </span>
                </Row>
                <ol className="flex flex-col gap-2">
                  {steps.map((step, index) => (
                    <PlanStepRow key={`${step.source}-${index}`} step={step} />
                  ))}
                </ol>
              </Stack>
            )
          })}
          <PlanFootnote
            artifacts={plan.artifact_patterns}
            envKeys={plan.env_keys}
          />
        </>
      ) : null}
    </Stack>
  )
}

function PlanStepRow({ step }: { step: PipelinePlanStep }) {
  return (
    <Stack render={<li />} gap={1.5} className="rounded-xl border px-3 py-2.5">
      <Row gap={2} className="flex-wrap">
        <span className="text-sm font-medium">{step.name}</span>
        {step.platform ? (
          <Badge variant="secondary">
            {BUILD_PLATFORM_LABELS[step.platform]}
          </Badge>
        ) : null}
        {step.flavor ? <Badge variant="outline">{step.flavor}</Badge> : null}
        {step.test_report ? <Badge variant="outline">Tests</Badge> : null}
        <Text variant="mono" render={<span />} className="ml-auto">
          {step.source}
        </Text>
      </Row>
      <Mono
        render={<pre />}
        size="xs"
        className="wrap-break-word whitespace-pre-wrap text-muted-foreground"
      >
        {step.run}
      </Mono>
    </Stack>
  )
}

function PlanFootnote({
  artifacts,
  envKeys,
}: {
  artifacts: Array<string>
  envKeys: Array<string>
}) {
  return (
    <Stack gap={1}>
      <Text variant="caption">
        Env: {envKeys.length > 0 ? <Mono>{envKeys.join(', ')}</Mono> : 'none'}
      </Text>
      <Text variant="caption">
        Artifacts:{' '}
        {artifacts.length > 0 ? <Mono>{artifacts.join(', ')}</Mono> : 'none'}
      </Text>
    </Stack>
  )
}
