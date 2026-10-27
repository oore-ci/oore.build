import type { ReactNode } from 'react'
import type { UseFormReturn } from 'react-hook-form'
import type { TriggerBuildForm } from '@/components/trigger-build-form'

import type {
  PipelineWorkflowSummary,
  PipelineWorkflowsResponse,
} from '@oore/client/models'
import { getApiErrorMessage } from '@/lib/api-client/api-error'
import { BUILD_PLATFORM_LABELS } from '@/lib/build-platforms'
import {
  FormControl,
  FormDescription,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
} from '@/components/ui/form'
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Mono } from '@/components/layout/mono'
import { Stack } from '@/components/layout/stack'
import { Text } from '@/components/layout/text'

function platformList(workflow: PipelineWorkflowSummary): string {
  return workflow.platforms
    .map((platform) => BUILD_PLATFORM_LABELS[platform])
    .join(', ')
}

function triggerSummary(workflow: PipelineWorkflowSummary): string {
  if (workflow.uses_pipeline_triggers) return "This pipeline's trigger settings"
  if (workflow.triggers.length === 0) return 'Only when started by hand'
  return workflow.triggers.join('; ')
}

/** Platforms, triggers and store uploads of the picked workflow. */
function WorkflowDetails({ workflow }: { workflow: PipelineWorkflowSummary }) {
  const rows: Array<[string, ReactNode]> = [
    ['Builds', platformList(workflow) || 'Unknown'],
    ['Starts on', triggerSummary(workflow)],
  ]
  if (workflow.publishes.length > 0) {
    rows.push([
      'Publishes',
      workflow.publishes.map((line) => <div key={line}>{line}</div>),
    ])
  }
  return (
    <Stack gap={1}>
      <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-xs">
        {rows.map(([label, value]) => (
          <div key={label} className="contents">
            <dt className="text-muted-foreground">{label}</dt>
            <dd>{value}</dd>
          </div>
        ))}
      </dl>
      {workflow.errors?.length ? (
        <Text variant="caption" className="text-destructive">
          {workflow.errors.join('; ')}
        </Text>
      ) : null}
    </Stack>
  )
}

/** Why no workflow can be picked at this ref, or undefined when one can. */
function unavailableReason(
  data: PipelineWorkflowsResponse,
  reference: ReactNode,
): ReactNode {
  const path = data.path ? <Mono>{data.path}</Mono> : 'The pipeline file'
  const errors = data.errors ?? []
  switch (data.state) {
    case 'missing':
      return (
        <>
          No pipeline file on {reference}, so the build uses this pipeline's
          settings.
        </>
      )
    case 'invalid':
      return (
        <>
          {path} on {reference} has errors, so the build will fail until they
          are fixed: {errors.join('; ')}
        </>
      )
    case 'unavailable':
      return (
        <>
          Could not read the workflows on {reference}
          {errors.length > 0 ? `: ${errors.join('; ')}` : ''}. The build checks
          the file when it starts.
        </>
      )
    case 'valid':
      return data.workflows.length === 0 ? (
        <>
          {path} on {reference} has no named workflows, so the build runs the
          whole file.
        </>
      ) : undefined
  }
}

/**
 * The Workflow field of the Run build drawer. Lists the workflows the
 * pipeline's file defines at the branch or commit being built, and says
 * plainly when there is nothing to pick.
 */
export function TriggerBuildWorkflowField({
  form,
  data,
  pending,
  error,
  reference,
}: {
  form: UseFormReturn<TriggerBuildForm>
  data?: PipelineWorkflowsResponse
  pending: boolean
  error: unknown
  reference: string
}) {
  const refLabel = reference ? <Mono>{reference}</Mono> : 'the default branch'
  let status: ReactNode
  if (pending) {
    status = <>Reading workflows on {refLabel}…</>
  } else if (error) {
    status = (
      <>
        Could not read the workflows on {refLabel}:{' '}
        {getApiErrorMessage(error, {})}. The build uses the pipeline's default.
      </>
    )
  } else if (data) {
    status = unavailableReason(data, refLabel)
  }

  if (status || !data) {
    return (
      <div className="space-y-1">
        <p className="text-sm font-medium">Workflow</p>
        <Text variant="caption">{status}</Text>
      </div>
    )
  }

  const workflows = data.workflows
  const staleDefault =
    data.default_workflow &&
    !workflows.some((workflow) => workflow.name === data.default_workflow)
      ? data.default_workflow
      : undefined
  const items = Object.fromEntries(
    workflows.map((workflow) => [
      workflow.name,
      workflow.name === data.default_workflow
        ? `${workflow.name} (default)`
        : workflow.name,
    ]),
  )
  return (
    <FormField
      control={form.control}
      name="workflow"
      render={({ field }) => {
        const chosen = workflows.find(
          (workflow) => workflow.name === field.value,
        )
        return (
          <FormItem>
            <FormLabel>Workflow</FormLabel>
            <Select
              value={field.value || null}
              onValueChange={(value) => {
                field.onChange(value ?? '')
                form.clearErrors('workflow')
                // Platforms follow the workflow; empty means all of them.
                form.setValue('platforms', [], { shouldDirty: false })
              }}
              items={items}
            >
              <FormControl>
                <SelectTrigger className="w-full" aria-label="Workflow">
                  <SelectValue placeholder="Choose a workflow" />
                </SelectTrigger>
              </FormControl>
              <SelectContent>
                <SelectGroup>
                  {workflows.map((workflow) => (
                    <SelectItem key={workflow.name} value={workflow.name}>
                      <span>{items[workflow.name]}</span>
                      <Text variant="caption" render={<span />}>
                        {platformList(workflow)}
                      </Text>
                    </SelectItem>
                  ))}
                </SelectGroup>
              </SelectContent>
            </Select>
            <FormDescription>
              {staleDefault ? (
                <>
                  The default workflow {staleDefault} is not in{' '}
                  <Mono>{data.path}</Mono> on {refLabel}; pick one to build.
                </>
              ) : data.default_workflow ||
                workflows.length === 1 ||
                field.value ? (
                <>
                  Defined in <Mono>{data.path}</Mono> on {refLabel}.
                </>
              ) : (
                <>
                  <Mono>{data.path}</Mono> on {refLabel} defines several
                  workflows and this pipeline has no default; pick one.
                </>
              )}
            </FormDescription>
            {chosen ? <WorkflowDetails workflow={chosen} /> : null}
            <FormMessage />
          </FormItem>
        )
      }}
    />
  )
}
