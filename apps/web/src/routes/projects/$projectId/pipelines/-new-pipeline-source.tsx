import { useEffect, useState } from 'react'

import type { RepositoryWorkflowPreview } from '@oore/client/models'
import type { PipelineFormValues } from '@/lib/pipeline-schema'
import { useRepositoryWorkflows, usePipelinePlan } from '@/hooks/use-pipelines'
import { getApiErrorMessage } from '@/lib/api-client/api-error'
import { repositoryFileStatus } from '@/lib/pipeline-form-utils'
import { planConfigFromForm } from '@/lib/pipeline-yaml-export'
import { planLine } from '@/lib/new-pipeline'
import { PlanResult } from '@/components/pipeline-plan-panel'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import {
  Item,
  ItemContent,
  ItemDescription,
  ItemTitle,
} from '@/components/ui/item'
import { RadioGroup, RadioGroupItem } from '@/components/ui/radio-group'
import { Mono } from '@/components/layout/mono'
import { Row } from '@/components/layout/row'
import { Stack } from '@/components/layout/stack'
import { Text } from '@/components/layout/text'

/** The "no file, use defaults" choice. */
export const NO_FILE = '__no_file__'

/** One thing a new pipeline can build from. */
export interface SourceOption {
  key: string
  /** The repository file; none for the defaults. */
  file?: RepositoryWorkflowPreview
  inUse?: boolean
  values: PipelineFormValues
}

/**
 * Step one of New pipeline: the files found in the repository, or none.
 * Each says in one line what it builds; "See the plan" opens the steps in
 * place.
 */
export function NewPipelineSource({
  invalidFiles,
  lookup,
  onPick,
  options,
  selected,
}: {
  invalidFiles: Array<RepositoryWorkflowPreview>
  lookup?: {
    projectId: string
    onFound: (file: RepositoryWorkflowPreview, reference: string) => void
  }
  onPick: (key: string) => void
  options: Array<SourceOption>
  selected: SourceOption
}) {
  const [planOpen, setPlanOpen] = useState(false)
  const [lookupOpen, setLookupOpen] = useState(false)

  return (
    <Stack gap={3}>
      <RadioGroup
        aria-label="Build from"
        value={selected.key}
        onValueChange={(value) => onPick(String(value))}
      >
        {options.map((option) => (
          <Item
            key={option.key}
            variant="outline"
            size="sm"
            render={
              <label
                htmlFor={`new-pipeline-source-${option.key}`}
                className="cursor-pointer"
              />
            }
            className="items-start has-data-checked:border-primary has-data-checked:bg-accent"
          >
            <RadioGroupItem
              id={`new-pipeline-source-${option.key}`}
              value={option.key}
              className="mt-0.5"
            />
            <ItemContent className="min-w-0">
              <ItemTitle className="w-full min-w-0">
                {option.file ? (
                  <Mono className="truncate">{option.file.path}</Mono>
                ) : (
                  'No file, use defaults'
                )}
                {option.inUse ? (
                  <Badge variant="outline" className="ms-auto">
                    In use
                  </Badge>
                ) : null}
              </ItemTitle>
              <ItemDescription>{planLine(option.values)}</ItemDescription>
            </ItemContent>
          </Item>
        ))}
      </RadioGroup>

      {invalidFiles.map((file) => (
        <Text key={file.path} variant="caption" className="text-destructive">
          <Mono>{file.path}</Mono> has errors, so it is not offered:{' '}
          {file.errors.join('; ')}
        </Text>
      ))}

      <Row gap={4} className="flex-wrap">
        <Button
          type="button"
          variant="link"
          size="sm"
          className="h-auto px-0"
          aria-expanded={planOpen}
          onClick={() => setPlanOpen((open) => !open)}
        >
          {planOpen ? 'Hide the plan' : 'See the plan'}
        </Button>
        {lookup && !lookupOpen ? (
          <Button
            type="button"
            variant="link"
            size="sm"
            className="h-auto px-0 text-muted-foreground"
            onClick={() => setLookupOpen(true)}
          >
            Use a file at another path
          </Button>
        ) : null}
      </Row>

      {planOpen ? <NewPipelinePlan values={selected.values} /> : null}
      {lookup && lookupOpen ? (
        <FileLookup
          {...lookup}
          onClose={() => setLookupOpen(false)}
          onFound={(file, reference) => {
            lookup.onFound(file, reference)
            setLookupOpen(false)
          }}
        />
      ) : null}
    </Stack>
  )
}

/** The steps a build of the picked source runs, from the daemon's planner. */
function NewPipelinePlan({ values }: { values: PipelineFormValues }) {
  const body = {
    execution_config: planConfigFromForm(values),
    build_play_bundle: false,
  }
  const planQuery = usePipelinePlan(body)
  return (
    <div className="rounded-lg border p-4">
      <PlanResult
        empty={false}
        error={planQuery.isError ? planQuery.error : null}
        loading={planQuery.isPending}
        result={planQuery.data}
      />
    </div>
  )
}

/** Finds a file outside `.oore*`, such as `ci/oore.yaml`, by its path. */
function FileLookup({
  onClose,
  onFound,
  projectId,
}: {
  onClose: () => void
  onFound: (file: RepositoryWorkflowPreview, reference: string) => void
  projectId: string
}) {
  const [draft, setDraft] = useState('')
  const [path, setPath] = useState('')
  const query = useRepositoryWorkflows(
    projectId,
    { path: path || undefined },
    { enabled: !!path },
  )
  const status = query.data
    ? repositoryFileStatus(query.data.workflows, { config_path: path })
    : undefined
  const found = status?.kind === 'ready' ? status.file : undefined
  const reference = query.data?.reference

  useEffect(() => {
    if (found && reference) onFound(found, reference)
  }, [found, reference, onFound])

  return (
    <Stack gap={2}>
      <Row gap={2}>
        <Input
          aria-label="File path"
          value={draft}
          placeholder="ci/oore.yaml"
          autoFocus
          spellCheck={false}
          autoCapitalize="off"
          className="font-mono"
          onChange={(event) => setDraft(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === 'Enter') {
              // Enter finds the file; it must not create the pipeline.
              event.preventDefault()
              setPath(draft.trim())
            }
            if (event.key === 'Escape') onClose()
          }}
        />
        <Button
          type="button"
          variant="outline"
          disabled={
            !draft.trim() || (query.isFetching && path === draft.trim())
          }
          onClick={() => setPath(draft.trim())}
        >
          Find
        </Button>
        <Button type="button" variant="ghost" onClick={onClose}>
          Cancel
        </Button>
      </Row>
      {query.isError ? (
        <Text variant="caption" className="text-destructive">
          Could not read the repository: {getApiErrorMessage(query.error, {})}
        </Text>
      ) : status?.kind === 'missing' ? (
        <Text variant="caption" className="text-destructive">
          <Mono>{status.path}</Mono> is not on <Mono>{reference}</Mono>.
        </Text>
      ) : status?.kind === 'invalid' ? (
        <Text variant="caption" className="text-destructive">
          <Mono>{status.path}</Mono> has errors: {status.errors.join('; ')}
        </Text>
      ) : null}
    </Stack>
  )
}
