import { useState } from 'react'
import type { ReactNode } from 'react'
import { useFormContext, useWatch } from 'react-hook-form'

import type { RepositoryWorkflowPreview } from '@oore/client/models'
import type { PipelineFormValues } from '@/lib/pipeline-schema'
import { useDebouncedJson } from '@/hooks/use-debounced-json'
import { useRepositoryWorkflows } from '@/hooks/use-pipelines'
import { getApiErrorMessage } from '@/lib/api-client/api-error'
import { repositoryFileStatus } from '@/lib/pipeline-form-utils'
import { RepositoryWorkflowSummary } from '@/components/repository-workflow-summary'
import { Button } from '@/components/ui/button'
import {
  FormControl,
  FormDescription,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
} from '@/components/ui/form'
import { Input } from '@/components/ui/input'
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Mono } from '@/components/layout/mono'
import { Row } from '@/components/layout/row'
import { Stack } from '@/components/layout/stack'
import { Text } from '@/components/layout/text'

/** The workflows a pipeline's `.oore.yaml` version 2 file defines. */
export interface PipelineWorkflowSource {
  path: string
  workflows: Array<string>
}

/** Read the pipeline's file from the repository as the form changes. */
export interface PipelineWorkflowLookup {
  projectId: string
  /** The project's default branch, read when no branch is typed. */
  defaultBranch?: string | null
  /** Shown as "Use this file" when a typed config path is found. */
  onUseFile?: (file: RepositoryWorkflowPreview, reference: string) => void
}

/** Typing a path or branch settles for this long before the file is read. */
const LOOKUP_DEBOUNCE_MS = 400

// Base UI selects treat an empty value as unselected.
const DEFAULT_WORKFLOW = '__default__'

/**
 * The Workflow selector. With `unavailable` set, the file cannot offer
 * workflows and the field says why; a saved workflow stays selectable so
 * it can be cleared.
 */
export function WorkflowField({
  source,
  unavailable,
  footer,
}: {
  source: PipelineWorkflowSource
  unavailable?: ReactNode
  footer?: ReactNode
}) {
  const form = useFormContext<PipelineFormValues>()
  const current = useWatch({ control: form.control, name: 'workflow' }) ?? ''
  const workflows = unavailable ? [] : source.workflows
  // Keep a saved workflow the file no longer defines visible and selected.
  const names =
    current && !workflows.includes(current)
      ? [...workflows, current]
      : workflows
  const items = {
    [DEFAULT_WORKFLOW]:
      workflows.length === 1
        ? `File default (${workflows[0]})`
        : workflows.length > 1
          ? 'Not set (choose when running; triggers start every matching workflow)'
          : 'Not set',
    ...Object.fromEntries(names.map((name) => [name, name])),
  }
  return (
    <FormField
      control={form.control}
      name="workflow"
      render={({ field }) => (
        <FormItem>
          <FormLabel>Default workflow</FormLabel>
          <Select
            value={field.value || DEFAULT_WORKFLOW}
            onValueChange={(value) =>
              field.onChange(value === DEFAULT_WORKFLOW ? '' : value)
            }
            items={items}
            disabled={!!unavailable && !current}
          >
            <FormControl>
              <SelectTrigger className="w-full" aria-label="Default workflow">
                <SelectValue />
              </SelectTrigger>
            </FormControl>
            <SelectContent>
              <SelectGroup>
                {Object.entries(items).map(([value, label]) => (
                  <SelectItem key={value} value={value}>
                    {label}
                  </SelectItem>
                ))}
              </SelectGroup>
            </SelectContent>
          </Select>
          <FormDescription>
            {unavailable ? (
              unavailable
            ) : (
              <>
                {workflows.length > 1 ? (
                  <>
                    <Mono>{source.path}</Mono> defines several workflows.
                    Triggers start only the default; without one, they start
                    every workflow that matches and Run build asks which to
                    build.
                  </>
                ) : (
                  <>
                    This pipeline builds the workflow in{' '}
                    <Mono>{source.path}</Mono>.
                  </>
                )}{' '}
                A workflow that sets <Mono>on</Mono> replaces the triggers below
                for pushes, pull requests and tags.
              </>
            )}
          </FormDescription>
          {!unavailable && current && !workflows.includes(current) ? (
            <Text variant="caption" className="text-destructive">
              {source.path} no longer defines {current}; builds that do not name
              a workflow will fail until you pick another default.
            </Text>
          ) : null}
          {footer}
          <FormMessage />
        </FormItem>
      )}
    />
  )
}

/**
 * The Workflow field for a form that edits where its config comes from:
 * reads the config path from the repository as it is typed, on the
 * default branch or one the user names, and says why nothing can be
 * picked instead of hiding the field.
 */
export function WorkflowFileLookup({
  projectId,
  defaultBranch,
  onUseFile,
}: PipelineWorkflowLookup) {
  const form = useFormContext<PipelineFormValues>()
  const [mode, configPath, current] = useWatch({
    control: form.control,
    name: ['config_mode', 'config_path', 'workflow'],
  })
  const explicit = mode === 'explicit'
  const [branchOpen, setBranchOpen] = useState(false)
  const [branchDraft, setBranchDraft] = useState('')
  const wanted = {
    path: explicit ? (configPath ?? '').trim() : '',
    ref: branchDraft.trim(),
  }
  const [lookup, setLookup] = useState(wanted)
  useDebouncedJson(wanted, LOOKUP_DEBOUNCE_MS, setLookup)
  const needsPath = explicit && !lookup.path
  const query = useRepositoryWorkflows(
    projectId,
    // Unset query values are left out of the request.
    { path: lookup.path || undefined, ref: lookup.ref || undefined },
    { enabled: !needsPath },
  )
  const branch =
    query.data?.reference || lookup.ref || defaultBranch || 'the default branch'

  let unavailable: ReactNode
  let found: RepositoryWorkflowPreview | undefined
  let hidden = false
  let path = lookup.path
  if (needsPath) {
    unavailable = 'Enter a config path to pick a workflow.'
  } else if (query.isPending) {
    unavailable = (
      <>
        Reading {path ? <Mono>{path}</Mono> : 'the repository'} from{' '}
        <Mono>{branch}</Mono>…
      </>
    )
  } else if (query.isError) {
    unavailable = (
      <>
        Could not read <Mono>{branch}</Mono>:{' '}
        {getApiErrorMessage(query.error, {})}
      </>
    )
  } else {
    const status = repositoryFileStatus(query.data.workflows, {
      // Without a path the runner reads `.oore.yaml`, then `.oore.yml`.
      config_path: explicit ? lookup.path : '.oore.yaml',
    })
    path = status.path
    if (status.kind === 'missing') {
      // A pipeline without a file builds from its own settings.
      hidden = !explicit && !current
      unavailable = explicit ? (
        <>
          <Mono>{status.path}</Mono> is not on <Mono>{branch}</Mono>.
        </>
      ) : (
        <>
          No <Mono>.oore.yaml</Mono> or <Mono>.oore.yml</Mono> on{' '}
          <Mono>{branch}</Mono>.
        </>
      )
    } else if (status.kind === 'invalid') {
      unavailable = (
        <>
          <Mono>{status.path}</Mono> on <Mono>{branch}</Mono> has errors:{' '}
          {status.errors.join('; ')}
        </>
      )
    } else {
      found = status.file
      if (status.file.workflows.length === 0) {
        unavailable = (
          <>
            <Mono>{status.path}</Mono> on <Mono>{branch}</Mono> defines no
            workflows.
          </>
        )
      }
    }
  }

  function closeBranch() {
    setBranchOpen(false)
    setBranchDraft('')
  }

  const branchControl = (
    <Row gap={2} className="flex-wrap">
      {branchOpen ? (
        <>
          <Text variant="caption" render={<label htmlFor="workflow-branch" />}>
            Read workflows from
          </Text>
          <Input
            id="workflow-branch"
            value={branchDraft}
            placeholder={defaultBranch ?? 'main'}
            autoFocus
            spellCheck={false}
            autoCapitalize="off"
            className="h-8 w-48 font-mono text-xs"
            onChange={(event) => setBranchDraft(event.target.value)}
            onKeyDown={(event) => {
              // The field is inside the pipeline form; Enter must not save it.
              if (event.key === 'Enter') event.preventDefault()
              if (event.key === 'Escape') closeBranch()
            }}
          />
          <Button type="button" variant="ghost" size="sm" onClick={closeBranch}>
            Use {defaultBranch ?? 'default branch'}
          </Button>
        </>
      ) : (
        <>
          <Text variant="caption">
            {hidden ? (
              unavailable
            ) : (
              <>
                Workflows read from <Mono>{branch}</Mono>
              </>
            )}
          </Text>
          <Button
            type="button"
            variant="link"
            size="sm"
            className="h-auto px-0 text-xs"
            onClick={() => {
              setBranchDraft(lookup.ref)
              setBranchOpen(true)
            }}
          >
            Change branch
          </Button>
        </>
      )}
    </Row>
  )

  if (hidden) return branchControl

  const preview =
    explicit && found && onUseFile && query.data
      ? { file: found, reference: query.data.reference, use: onUseFile }
      : undefined

  return (
    <Stack gap={4}>
      {preview ? (
        <Stack gap={3}>
          <RepositoryWorkflowSummary
            workflow={preview.file}
            reference={preview.reference}
          />
          <div>
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={() => preview.use(preview.file, preview.reference)}
            >
              Use this file's settings
            </Button>
          </div>
        </Stack>
      ) : null}
      <WorkflowField
        source={{ path, workflows: found?.workflows ?? [] }}
        unavailable={unavailable}
        footer={branchControl}
      />
    </Stack>
  )
}
