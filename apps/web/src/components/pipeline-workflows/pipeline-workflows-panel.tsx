import { useMemo, useState, type ReactNode } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import { Alert02Icon, WorkflowSquare03Icon } from '@hugeicons/core-free-icons'

import type {
  PipelineWorkflowsResponse,
  SigningIdentity,
} from '@oore/client/models'
import { usePipelineWorkflows } from '@/hooks/use-pipelines'
import {
  usePipelineWorkflowSettings,
  useSigningIdentities,
} from '@/hooks/use-signing-identities'
import { BUILD_PLATFORM_LABELS } from '@/lib/build-platforms'
import {
  signingPlatforms,
  workflowRows,
  type WorkflowRow,
} from '@/lib/pipeline-workflow-rows'
import { CollectionError } from '@/components/collection'
import {
  DataTable,
  useDataTable,
  type DataTableColumnDef,
} from '@/components/data-table'
import { SettingsSection } from '@/components/settings/settings-section'
import { EmptyValue } from '@/components/table-values'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from '@/components/ui/empty'
import { Skeleton } from '@/components/ui/skeleton'
import { Mono } from '@/components/layout/mono'
import { Row } from '@/components/layout/row'
import { Stack } from '@/components/layout/stack'
import { Text } from '@/components/layout/text'
import { WorkflowSettingsDrawer } from './workflow-settings-drawer'

const WORKFLOWS_DOCS_URL =
  'https://docs.oore.build/reference/config/oore-yaml-v2#signing'

function IdentityValue({
  id,
  identities,
}: {
  id?: string | null
  identities: Map<string, SigningIdentity>
}) {
  if (!id)
    return (
      <Text variant="muted" render={<span />}>
        From the file
      </Text>
    )
  const identity = identities.get(id)
  if (!identity) {
    return <span className="text-destructive">Deleted identity</span>
  }
  return <Mono>{identity.name}</Mono>
}

function publishingValue(row: WorkflowRow): ReactNode {
  if (row.settings?.publish_enabled === false) {
    return <Badge variant="outline">Off</Badge>
  }
  const count = row.summary?.publishes.length ?? 0
  if (count === 0)
    return (
      <Text variant="muted" render={<span />}>
        None in the file
      </Text>
    )
  return count === 1 ? '1 upload' : `${count} uploads`
}

function approvalValue(row: WorkflowRow): ReactNode {
  const required = row.settings?.approval_required
  if (required === true) return 'Required'
  if (required === false) return 'Not required'
  return (
    <Text variant="muted" render={<span />}>
      Pipeline policy
    </Text>
  )
}

function WorkflowsUnavailable({
  listing,
}: {
  listing: PipelineWorkflowsResponse
}) {
  const path = listing.path ? <Mono>{listing.path}</Mono> : 'The pipeline file'
  const on = listing.reference ? (
    <>
      {' '}
      on <Mono>{listing.reference}</Mono>
    </>
  ) : null
  if (listing.state === 'invalid' || listing.state === 'unavailable') {
    return (
      <Alert variant="destructive">
        <HugeiconsIcon icon={Alert02Icon} size={16} />
        <AlertTitle>
          {listing.state === 'invalid'
            ? 'The pipeline file has errors'
            : 'The pipeline file could not be read'}
        </AlertTitle>
        <AlertDescription>
          {path}
          {on}: {(listing.errors ?? []).join('; ') || 'Unknown error'}
        </AlertDescription>
      </Alert>
    )
  }
  return (
    <Empty className="border bg-card p-6">
      <EmptyHeader>
        <EmptyMedia variant="icon">
          <HugeiconsIcon icon={WorkflowSquare03Icon} />
        </EmptyMedia>
        <EmptyTitle>No workflows</EmptyTitle>
        <EmptyDescription>
          {listing.state === 'missing' ? (
            <>No pipeline file{on}, so there is nothing to set per workflow. </>
          ) : (
            <>
              {path}
              {on} defines no workflows, so its builds use the pipeline’s
              settings.{' '}
            </>
          )}
          <a
            href={WORKFLOWS_DOCS_URL}
            target="_blank"
            rel="noopener noreferrer"
            className="underline underline-offset-4"
          >
            Learn more
          </a>
        </EmptyDescription>
      </EmptyHeader>
    </Empty>
  )
}

/**
 * The pipeline's workflows with what each signs and publishes with, and a
 * Settings drawer per workflow for choices that replace the file's.
 */
export function PipelineWorkflowsPanel({
  canChangeApproval,
  canWrite,
  pipelineId,
  projectId,
}: {
  canChangeApproval: boolean
  canWrite: boolean
  pipelineId: string
  projectId: string
}) {
  const workflowsQuery = usePipelineWorkflows(pipelineId)
  const settingsQuery = usePipelineWorkflowSettings(pipelineId)
  const identitiesQuery = useSigningIdentities(projectId)
  const [editing, setEditing] = useState<string>()

  const identities = useMemo(
    () =>
      new Map(
        (identitiesQuery.data ?? []).map((identity) => [identity.id, identity]),
      ),
    [identitiesQuery.data],
  )
  const rows = useMemo(
    () => workflowRows(workflowsQuery.data, settingsQuery.data ?? []),
    [workflowsQuery.data, settingsQuery.data],
  )
  const columns = useMemo<Array<DataTableColumnDef<WorkflowRow>>>(
    () => [
      {
        accessorKey: 'name',
        header: 'Workflow',
        cell: ({ row }) => (
          <Row gap={2} className="flex-wrap">
            <Mono className="font-medium">{row.original.name}</Mono>
            {row.original.isDefault ? (
              <Badge variant="secondary">Default</Badge>
            ) : null}
            {row.original.summary ? null : (
              <Badge variant="outline">Not in the file</Badge>
            )}
          </Row>
        ),
        enableSorting: false,
      },
      {
        id: 'builds',
        header: 'Builds',
        cell: ({ row }) => {
          const platforms = row.original.summary?.platforms ?? []
          return platforms.length > 0 ? (
            platforms.map((p) => BUILD_PLATFORM_LABELS[p]).join(', ')
          ) : (
            <EmptyValue />
          )
        },
        enableSorting: false,
      },
      {
        id: 'signing',
        header: 'Signing',
        cell: ({ row }) => (
          <Stack gap={0.5} className="text-sm">
            {signingPlatforms(row.original).map((platform) => (
              <div key={platform}>
                <span className="text-muted-foreground">
                  {BUILD_PLATFORM_LABELS[platform]}:{' '}
                </span>
                <IdentityValue
                  id={
                    platform === 'android'
                      ? row.original.settings?.android_identity_id
                      : row.original.settings?.ios_identity_id
                  }
                  identities={identities}
                />
              </div>
            ))}
          </Stack>
        ),
        enableSorting: false,
      },
      {
        id: 'publishing',
        header: 'Publishing',
        cell: ({ row }) => publishingValue(row.original),
        enableSorting: false,
      },
      {
        id: 'approval',
        header: 'Approval',
        cell: ({ row }) => approvalValue(row.original),
        enableSorting: false,
      },
      {
        id: 'actions',
        header: () => <span className="sr-only">Actions</span>,
        cell: ({ row }) => (
          <Button
            variant="outline"
            size="sm"
            onClick={() => setEditing(row.original.name)}
          >
            {canWrite ? 'Settings' : 'View'}
            <span className="sr-only"> for {row.original.name}</span>
          </Button>
        ),
        enableSorting: false,
      },
    ],
    [canWrite, identities],
  )
  const table = useDataTable({
    columns,
    data: rows,
    getRowId: (row) => row.name,
  })

  const listing = workflowsQuery.data
  const editingRow = rows.find((row) => row.name === editing)
  const error = workflowsQuery.error ?? settingsQuery.error
  const loading = workflowsQuery.isLoading || settingsQuery.isLoading

  return (
    <SettingsSection
      title="Workflows"
      hint="Choices here replace what the file says for this pipeline’s builds."
      meta={
        listing?.path ? (
          <Text variant="caption">
            <Mono>{listing.path}</Mono>
            {listing.reference ? (
              <>
                {' '}
                on <Mono>{listing.reference}</Mono>
              </>
            ) : null}
          </Text>
        ) : undefined
      }
    >
      {error ? (
        <CollectionError
          title="Failed to load workflows"
          description={error.message}
          onRetry={() => {
            void workflowsQuery.refetch()
            void settingsQuery.refetch()
          }}
        />
      ) : loading ? (
        <Skeleton className="h-40 w-full" />
      ) : rows.length === 0 && listing ? (
        <WorkflowsUnavailable listing={listing} />
      ) : (
        <Stack gap={3}>
          {listing && listing.state !== 'valid' ? (
            <WorkflowsUnavailable listing={listing} />
          ) : null}
          <DataTable table={table} />
          <Text variant="caption">
            Unset choices follow the file’s signing and publish settings, then
            the pipeline’s.
          </Text>
        </Stack>
      )}

      <WorkflowSettingsDrawer
        canChangeApproval={canChangeApproval}
        canWrite={canWrite}
        identities={identitiesQuery.data ?? []}
        identitiesError={identitiesQuery.error}
        onOpenChange={(open) => {
          if (!open) setEditing(undefined)
        }}
        pipelineId={pipelineId}
        projectId={projectId}
        row={editingRow}
      />
    </SettingsSection>
  )
}
