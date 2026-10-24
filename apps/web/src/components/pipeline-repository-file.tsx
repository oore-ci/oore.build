import { useEffect, useRef, useState } from 'react'
import { useFormContext } from 'react-hook-form'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  AlertCircleIcon,
  Alert02Icon,
  CheckmarkCircle02Icon,
  Copy01Icon,
  Download04Icon,
  PencilEdit02Icon,
} from '@hugeicons/core-free-icons'

import type {
  ExportPipelineYamlRequest,
  ImportPipelineYamlRequest,
} from '@oore/client/models'
import type { PipelineFormValues } from '@/lib/pipeline-schema'
import { useDebouncedJson } from '@/hooks/use-debounced-json'
import {
  usePipelineYamlExport,
  usePipelineYamlImport,
} from '@/hooks/use-pipelines'
import { getApiErrorMessage } from '@/lib/api-client/api-error'
import {
  downloadFileName,
  repositoryFileView,
} from '@/lib/pipeline-yaml-export'
import type { ImportOutcome } from '@/lib/pipeline-yaml-import'
import {
  forEachField,
  importOutcome,
  yamlImportRequest,
} from '@/lib/pipeline-yaml-import'
import { toast } from '@/lib/toast'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { Skeleton } from '@/components/ui/skeleton'
import { Spinner } from '@/components/ui/spinner'
import { Textarea } from '@/components/ui/textarea'
import { Mono } from '@/components/layout/mono'
import { Row } from '@/components/layout/row'
import { Stack } from '@/components/layout/stack'
import { Text } from '@/components/layout/text'

/** Edits settle for this long before the file is generated again. */
const EXPORT_DEBOUNCE_MS = 400
/** Typing in the file settles for this long before it is read back. */
const IMPORT_DEBOUNCE_MS = 500

function downloadYaml(yaml: string, path: string) {
  const blob = new Blob([yaml], { type: 'application/yaml' })
  const url = URL.createObjectURL(blob)
  const anchor = document.createElement('a')
  anchor.href = url
  anchor.download = downloadFileName(path)
  anchor.click()
  // Some browsers start the download after `click` returns.
  setTimeout(() => URL.revokeObjectURL(url), 0)
}

/**
 * "Repository file": the form's settings as a `version: 2` `.oore.yaml`,
 * generated and checked by the daemon, ready to copy or download and commit.
 * In edit mode the file is the source: edits are read back by the daemon
 * and, when the form can hold all of the file, update the form in place.
 */
export function PipelineRepositoryFile({
  manualOnlyTriggers = false,
  path,
  request,
  sharedWith = [],
  values,
}: {
  /** The repository only builds manually, so the form has no `on`. */
  manualOnlyTriggers?: boolean
  /** Where the runner looks for the file in the repository. */
  path: string
  request: ExportPipelineYamlRequest
  /** Other pipelines in the project that read the same path. */
  sharedWith?: ReadonlyArray<string>
  /** The form's values, which edits to the file update. */
  values: PipelineFormValues
}) {
  const [body, setBody] = useState<ExportPipelineYamlRequest | null>(null)
  useDebouncedJson(request, EXPORT_DEBOUNCE_MS, setBody)
  const exportQuery = usePipelineYamlExport(body)
  const result = body ? exportQuery.data : undefined
  const view = repositoryFileView({ isError: exportQuery.isError, result })
  const generated = view.kind === 'ready' ? view.yaml : undefined

  const [editing, setEditing] = useState(false)
  const [draft, setDraft] = useState('')
  // The settings an edit is read against, taken when the text changes, so
  // the form changing (by an import or by hand) does not re-read the file.
  const [importBase, setImportBase] =
    useState<ExportPipelineYamlRequest | null>(null)
  const [importBody, setImportBody] =
    useState<ImportPipelineYamlRequest | null>(null)
  useDebouncedJson(
    editing && importBase ? yamlImportRequest(draft, importBase) : null,
    IMPORT_DEBOUNCE_MS,
    setImportBody,
  )
  const importQuery = usePipelineYamlImport(editing ? importBody : null)
  // Only an answer for the text on screen counts.
  const settled =
    editing &&
    importBody?.yaml === draft &&
    !importQuery.isPlaceholderData &&
    !importQuery.isFetching
  const outcome: ImportOutcome = importOutcome({
    isError: settled && importQuery.isError,
    result: settled ? importQuery.data : undefined,
    current: values,
    manualOnlyTriggers,
  })

  // Apply each new text once, so form edits made while the file is open are
  // not undone by the next read of the same text.
  const form = useFormContext<PipelineFormValues>()
  const appliedYaml = useRef<string | null>(null)
  useEffect(() => {
    if (!settled || !importBody || appliedYaml.current === importBody.yaml) {
      return
    }
    const next = importOutcome({
      isError: importQuery.isError,
      result: importQuery.data,
      current: form.getValues(),
      manualOnlyTriggers,
    })
    if (next.kind !== 'ready') return
    appliedYaml.current = importBody.yaml
    let count = 0
    forEachField(next.fields, () => count++)
    if (count === 0) return
    // Defaults stay, so the form is dirty against the saved pipeline.
    form.reset(
      { ...form.getValues(), ...next.fields },
      {
        keepDefaultValues: true,
        keepErrors: true,
        keepTouched: true,
        keepIsSubmitted: true,
      },
    )
    toast.success('Form updated from the file')
  }, [
    settled,
    importBody,
    importQuery.data,
    importQuery.isError,
    form,
    manualOnlyTriggers,
  ])

  function startEditing() {
    if (!generated) return
    appliedYaml.current = generated
    setDraft(generated)
    setImportBase(request)
    setEditing(true)
  }

  function stopEditing() {
    setEditing(false)
    setDraft('')
    setImportBase(null)
    setImportBody(null)
    appliedYaml.current = null
  }

  const yaml = editing ? draft : generated

  function copyYaml() {
    if (!yaml) return
    void navigator.clipboard.writeText(yaml).then(
      () => toast.success(`Copied ${downloadFileName(path)}`),
      () => toast.error('Could not copy the file'),
    )
  }

  return (
    <Stack gap={2} className="min-w-0">
      <Row gap={2} className="flex-wrap">
        <span className="text-sm font-medium">Repository file</span>
        <Mono size="xs" className="text-muted-foreground">
          {path}
        </Mono>
        <Row gap={1} className="ml-auto">
          {editing ? (
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={stopEditing}
            >
              Done
            </Button>
          ) : (
            <Button
              type="button"
              variant="ghost"
              size="sm"
              disabled={!generated}
              onClick={startEditing}
            >
              <HugeiconsIcon icon={PencilEdit02Icon} data-icon="inline-start" />
              Edit
            </Button>
          )}
          <Button
            type="button"
            variant="ghost"
            size="sm"
            disabled={!yaml}
            onClick={copyYaml}
          >
            <HugeiconsIcon icon={Copy01Icon} data-icon="inline-start" />
            Copy
          </Button>
          <Button
            type="button"
            variant="ghost"
            size="sm"
            disabled={!yaml}
            onClick={() => yaml && downloadYaml(yaml, path)}
          >
            <HugeiconsIcon icon={Download04Icon} data-icon="inline-start" />
            Download
          </Button>
        </Row>
      </Row>
      {editing ? (
        <FileEditor
          draft={draft}
          error={importQuery.error}
          onChange={(next) => {
            setDraft(next)
            setImportBase(request)
          }}
          outcome={outcome}
        />
      ) : view.kind === 'error' ? (
        <Alert variant="destructive">
          <HugeiconsIcon icon={AlertCircleIcon} size={16} />
          <AlertDescription>
            Could not generate the file:{' '}
            {getApiErrorMessage(exportQuery.error, {})}
          </AlertDescription>
        </Alert>
      ) : view.kind === 'loading' ? (
        <Stack gap={2} aria-busy="true">
          <Skeleton className="h-4 w-40" />
          <Skeleton className="h-48 w-full" />
        </Stack>
      ) : view.kind === 'invalid' ? (
        <Alert variant="destructive">
          <HugeiconsIcon icon={AlertCircleIcon} size={16} />
          <AlertDescription>
            Fix these settings to generate the file:
            <ul className="mt-1 list-disc space-y-1 pl-4">
              {view.errors.map((message, index) => (
                <li key={index}>{message}</li>
              ))}
            </ul>
          </AlertDescription>
        </Alert>
      ) : (
        <>
          <Mono
            render={<pre />}
            size="xs"
            aria-label="Generated repository file"
            tabIndex={0}
            className="max-h-80 overflow-auto rounded-lg border bg-muted/40 p-3 lg:max-h-[36rem]"
          >
            {generated}
          </Mono>
          {view.warnings.length > 0 ? (
            <Alert>
              <HugeiconsIcon icon={Alert02Icon} size={16} />
              <AlertDescription>
                <ul className="list-disc space-y-1 pl-4">
                  {view.warnings.map((message, index) => (
                    <li key={index}>{message}</li>
                  ))}
                </ul>
              </AlertDescription>
            </Alert>
          ) : null}
        </>
      )}
      <Text variant="caption">
        Commit it as <Mono>{path}</Mono>. From the next build, every pipeline
        that reads <Mono>{path}</Mono> builds from this file and follows its{' '}
        <Mono>on</Mono>, and these Build settings stop applying. Env values are
        copied as written: keep secrets in Secrets and reference them as{' '}
        <Mono>{'${{ secrets.NAME }}'}</Mono>.
      </Text>
      {sharedWith.length > 0 ? (
        <Alert>
          <HugeiconsIcon icon={Alert02Icon} size={16} />
          <AlertDescription>
            {sharedWith.length === 1 ? 'Another pipeline' : 'Other pipelines'}{' '}
            in this project also read <Mono>{path}</Mono> and will switch to
            this file too: {sharedWith.join(', ')}.
          </AlertDescription>
        </Alert>
      ) : null}
      {!editing && result && result.notes.length > 0 ? (
        <ul className="list-disc space-y-1 pl-4">
          {result.notes.map((note, index) => (
            <Text key={index} variant="caption" render={<li />}>
              {note}
            </Text>
          ))}
        </ul>
      ) : null}
    </Stack>
  )
}

/** The file as a code text box, with what reading it back means for the form. */
function FileEditor({
  draft,
  error,
  onChange,
  outcome,
}: {
  draft: string
  error: Error | null
  onChange: (draft: string) => void
  outcome: ImportOutcome
}) {
  return (
    <Stack gap={2}>
      <Textarea
        aria-label="Repository file"
        aria-describedby="repository-file-status"
        value={draft}
        onChange={(event) => onChange(event.target.value)}
        spellCheck={false}
        autoCapitalize="off"
        autoCorrect="off"
        rows={20}
        wrap="off"
        className="max-h-[36rem] min-h-80 overflow-auto font-mono text-xs whitespace-pre"
      />
      <div id="repository-file-status" aria-live="polite">
        {outcome.kind === 'checking' ? (
          <Row gap={2}>
            <Spinner aria-hidden="true" />
            <Text variant="caption">Checking the file…</Text>
          </Row>
        ) : outcome.kind === 'error' ? (
          <Alert variant="destructive">
            <HugeiconsIcon icon={AlertCircleIcon} size={16} />
            <AlertDescription>
              Could not check the file: {getApiErrorMessage(error, {})}
            </AlertDescription>
          </Alert>
        ) : outcome.kind === 'invalid' ? (
          <Alert variant="destructive">
            <HugeiconsIcon icon={AlertCircleIcon} size={16} />
            <AlertDescription>
              Fix the file to update the form. The form is unchanged.
              <MessageList messages={outcome.errors} />
            </AlertDescription>
          </Alert>
        ) : outcome.kind === 'unsupported' ? (
          <Alert>
            <HugeiconsIcon icon={Alert02Icon} size={16} />
            <AlertDescription>
              The form cannot hold this file, so it is unchanged. Keep the file
              as YAML only, or change these parts:
              <MessageList messages={outcome.unsupported} />
            </AlertDescription>
          </Alert>
        ) : (
          <Row gap={2}>
            <HugeiconsIcon
              icon={CheckmarkCircle02Icon}
              size={16}
              className="text-success"
            />
            <Text variant="caption">The form matches the file.</Text>
          </Row>
        )}
      </div>
      {outcome.kind === 'ready' || outcome.kind === 'unsupported' ? (
        outcome.notes.length > 0 ? (
          <ul className="list-disc space-y-1 pl-4">
            {outcome.notes.map((note, index) => (
              <Text key={index} variant="caption" render={<li />}>
                {note}
              </Text>
            ))}
          </ul>
        ) : null
      ) : null}
      <Text variant="caption">
        Edits update the form, and the plan, once the file checks out. Comments
        and custom step names are not kept: Done shows the file generated from
        the form again.
      </Text>
    </Stack>
  )
}

function MessageList({ messages }: { messages: Array<string> }) {
  return (
    <ul className="mt-1 list-disc space-y-1 pl-4">
      {messages.map((message, index) => (
        <li key={index}>{message}</li>
      ))}
    </ul>
  )
}
