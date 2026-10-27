import { useState } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import { Add01Icon, Refresh01Icon } from '@hugeicons/core-free-icons'
import type { RepositoryConfigFile } from '@oore/client/models'

import {
  useCreatePipeline,
  useRefreshRepositoryConfigFiles,
  useRepositoryConfigFiles,
} from '@/hooks/use-pipelines'
import {
  addableConfigFiles,
  describeConfigFile,
  unusedConfigFiles,
} from '@/lib/repository-config-files'
import { relativeTime } from '@/lib/format-utils'
import { toast } from '@/lib/toast'
import {
  SettingsSection,
  SettingsSurface,
} from '@/components/settings/settings-section'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  Item,
  ItemActions,
  ItemContent,
  ItemDescription,
  ItemGroup,
  ItemSeparator,
  ItemTitle,
} from '@/components/ui/item'
import { Mono } from '@/components/layout/mono'
import { Text } from '@/components/layout/text'

/**
 * Config files on the default branch that no pipeline builds from, each one
 * click from becoming a pipeline. Renders nothing when every file is used.
 */
export function RepositoryConfigFilesSection({
  canWrite,
  projectId,
}: {
  canWrite: boolean
  projectId: string
}) {
  const filesQuery = useRepositoryConfigFiles(projectId)
  const refreshMutation = useRefreshRepositoryConfigFiles()
  const createMutation = useCreatePipeline()
  const [adding, setAdding] = useState<ReadonlySet<string>>(new Set())

  const data = filesQuery.data
  const unused = unusedConfigFiles(data)
  const addable = addableConfigFiles(data)
  if (!data || (unused.length === 0 && !data.error)) return null

  async function add(files: Array<RepositoryConfigFile>) {
    setAdding(new Set(files.map((file) => file.path)))
    let added = 0
    for (const file of files) {
      try {
        await createMutation.mutateAsync({
          projectId,
          data: { name: file.suggested_name, config_path: file.path },
        })
        added += 1
      } catch (error) {
        toast.error(
          `Failed to add ${file.path}: ${error instanceof Error ? error.message : String(error)}`,
        )
      }
    }
    setAdding(new Set())
    if (added > 0) {
      toast.success(added === 1 ? 'Pipeline added' : `${added} pipelines added`)
    }
  }

  function refresh() {
    refreshMutation.mutate(
      { projectId },
      {
        onError: (error) =>
          toast.error(`Failed to read the repository: ${error.message}`),
      },
    )
  }

  const busy = adding.size > 0

  return (
    <SettingsSection
      title="Found in repository"
      meta={<Badge variant="secondary">{unused.length}</Badge>}
      actions={
        canWrite ? (
          <>
            <Button
              variant="ghost"
              size="sm"
              disabled={refreshMutation.isPending}
              onClick={refresh}
            >
              <HugeiconsIcon icon={Refresh01Icon} data-icon="inline-start" />
              Refresh
            </Button>
            {addable.length > 1 ? (
              <Button
                variant="outline"
                size="sm"
                disabled={busy}
                onClick={() => void add(addable)}
              >
                Add all
              </Button>
            ) : null}
          </>
        ) : undefined
      }
    >
      <SettingsSurface inset={false}>
        {data.error ? (
          <Alert variant="destructive" className="m-4 w-auto sm:m-5">
            <AlertDescription>
              Oore could not read the repository: {data.error}
            </AlertDescription>
          </Alert>
        ) : null}
        <ItemGroup className="gap-0" role="list">
          {unused.flatMap((file, index) => {
            const row = (
              <Item key={file.path} size="sm" role="listitem">
                <ItemContent className="min-w-0">
                  <ItemTitle className="max-w-full">
                    <Mono className="truncate">{file.path}</Mono>
                    {file.valid ? null : (
                      <Badge variant="destructive">Invalid</Badge>
                    )}
                  </ItemTitle>
                  <ItemDescription className="wrap-break-word">
                    {describeConfigFile(file)}
                  </ItemDescription>
                </ItemContent>
                {canWrite ? (
                  <ItemActions>
                    <Button
                      variant="outline"
                      size="sm"
                      disabled={!file.valid || busy}
                      onClick={() => void add([file])}
                      aria-label={`Add a pipeline for ${file.path}`}
                    >
                      <HugeiconsIcon
                        icon={Add01Icon}
                        data-icon="inline-start"
                      />
                      Add
                    </Button>
                  </ItemActions>
                ) : null}
              </Item>
            )
            return index === 0
              ? [row]
              : [
                  <ItemSeparator
                    key={`separator-${file.path}`}
                    className="my-0"
                  />,
                  row,
                ]
          })}
        </ItemGroup>
      </SettingsSurface>
      {data.scanned_at ? (
        <Text variant="caption">
          Read from {data.branch} {relativeTime(data.scanned_at)}. Add makes a
          pipeline named after the file.
        </Text>
      ) : null}
    </SettingsSection>
  )
}
