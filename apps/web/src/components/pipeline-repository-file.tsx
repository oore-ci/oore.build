import { useState } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  AlertCircleIcon,
  Alert02Icon,
  Copy01Icon,
  Download04Icon,
} from '@hugeicons/core-free-icons'

import type { ExportPipelineYamlRequest } from '@oore/client/models'
import { useDebouncedJson } from '@/hooks/use-debounced-json'
import { usePipelineYamlExport } from '@/hooks/use-pipelines'
import { getApiErrorMessage } from '@/lib/api-client/api-error'
import {
  downloadFileName,
  repositoryFileView,
} from '@/lib/pipeline-yaml-export'
import { toast } from '@/lib/toast'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { Skeleton } from '@/components/ui/skeleton'
import { Mono } from '@/components/layout/mono'
import { Row } from '@/components/layout/row'
import { Stack } from '@/components/layout/stack'
import { Text } from '@/components/layout/text'

/** Edits settle for this long before the file is generated again. */
const EXPORT_DEBOUNCE_MS = 400

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
 */
export function PipelineRepositoryFile({
  path,
  request,
  sharedWith = [],
}: {
  /** Where the runner looks for the file in the repository. */
  path: string
  request: ExportPipelineYamlRequest
  /** Other pipelines in the project that read the same path. */
  sharedWith?: ReadonlyArray<string>
}) {
  const [body, setBody] = useState<ExportPipelineYamlRequest | null>(null)
  useDebouncedJson(request, EXPORT_DEBOUNCE_MS, setBody)
  const exportQuery = usePipelineYamlExport(body)
  const result = body ? exportQuery.data : undefined
  const view = repositoryFileView({ isError: exportQuery.isError, result })
  const yaml = view.kind === 'ready' ? view.yaml : undefined

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
      {view.kind === 'error' ? (
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
            {yaml}
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
      {result && result.notes.length > 0 ? (
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
