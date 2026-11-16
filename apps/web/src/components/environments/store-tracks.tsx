import { useState } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  Add01Icon,
  MoreHorizontalCircle01Icon,
} from '@hugeicons/core-free-icons'

import type { ProjectEnvironment, ReleaseStore } from '@oore/client/models'
import { getApiErrorMessage } from '@/lib/api-client/api-error'
import {
  approversSummary,
  environmentTargetLabel,
  rolloutPolicySummary,
} from '@/lib/environments'
import { toast } from '@/lib/toast'
import {
  useDeleteEnvironment,
  useProjectEnvironments,
} from '@/hooks/use-environments'
import ConfirmDialog from '@/components/ConfirmDialog'
import { EnvironmentDialog } from '@/components/environments/environment-dialog'
import { RowList } from '@/components/environments/row-list'
import { SectionHeading } from '@/components/settings/settings-section'
import { Text } from '@/components/layout/text'
import { Button } from '@/components/ui/button'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import {
  Item,
  ItemActions,
  ItemContent,
  ItemDescription,
  ItemTitle,
} from '@/components/ui/item'
import { Skeleton } from '@/components/ui/skeleton'

/** Where the track goes, without the store the section already names. */
function storeTrackLabel(environment: ProjectEnvironment) {
  const label = environmentTargetLabel(environment)
  return label.split(' · ').slice(1).join(' · ') || label
}

function TrackSetupRow({
  canManage,
  environment,
  onDelete,
  onEdit,
}: {
  canManage: boolean
  environment: ProjectEnvironment
  onDelete: () => void
  onEdit: () => void
}) {
  return (
    <Item size="sm" role="listitem" className="flex-wrap gap-y-2">
      <ItemContent className="min-w-48">
        <ItemTitle>{environment.name}</ItemTitle>
        <ItemDescription>
          {storeTrackLabel(environment)} ·{rolloutPolicySummary(environment)}
          {environment.approval_required
            ? ` · Approval: ${approversSummary(environment.approvers)}`
            : ''}
        </ItemDescription>
      </ItemContent>
      {canManage ? (
        <ItemActions>
          <Button size="sm" variant="outline" onClick={onEdit}>
            Edit
          </Button>
          <DropdownMenu>
            <DropdownMenuTrigger
              render={<Button variant="ghost" size="icon-sm" />}
            >
              <span className="sr-only">More for {environment.name}</span>
              <HugeiconsIcon icon={MoreHorizontalCircle01Icon} />
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" className="w-40">
              <DropdownMenuGroup>
                <DropdownMenuItem variant="destructive" onClick={onDelete}>
                  Delete track
                </DropdownMenuItem>
              </DropdownMenuGroup>
            </DropdownMenuContent>
          </DropdownMenu>
        </ItemActions>
      ) : null}
    </Item>
  )
}

/**
 * The tracks an app releases to in one store: where each goes, how it rolls
 * out and who approves promotions into it. Edits open in place.
 */
export function StoreTracks({
  canManage,
  projectId,
  store,
}: {
  /** Maintainers add, edit and delete tracks. */
  canManage: boolean
  projectId: string
  store: ReleaseStore
}) {
  const environmentsQuery = useProjectEnvironments(projectId)
  const deleteMutation = useDeleteEnvironment(projectId)
  const [createOpen, setCreateOpen] = useState(false)
  const [editing, setEditing] = useState<ProjectEnvironment>()
  const [deleting, setDeleting] = useState<ProjectEnvironment>()
  const tracks = (environmentsQuery.data?.environments ?? [])
    .filter((environment) => environment.store === store)
    .sort((a, b) => a.position - b.position)
  const headingId = `${store}-tracks`

  function handleDelete() {
    if (!deleting) return
    deleteMutation.mutate(deleting.id, {
      onSuccess: () => {
        toast.success(`Deleted ${deleting.name}`)
        setDeleting(undefined)
      },
      onError: (err) => toast.error(getApiErrorMessage(err, {})),
    })
  }

  return (
    <section aria-labelledby={headingId} className="flex flex-col gap-3">
      <div className="flex items-center justify-between gap-4">
        <SectionHeading
          as="h3"
          id={headingId}
          title="Tracks"
          hint="Promote a build along these in order. A track with approval waits for an approver."
          meta={tracks.length > 0 ? String(tracks.length) : undefined}
        />
        {canManage ? (
          <Button
            size="sm"
            variant="outline"
            onClick={() => setCreateOpen(true)}
          >
            <HugeiconsIcon icon={Add01Icon} />
            Add track
          </Button>
        ) : null}
      </div>
      {environmentsQuery.error ? (
        <Text variant="muted">
          Tracks could not be loaded. {environmentsQuery.error.message}
        </Text>
      ) : environmentsQuery.isLoading ? (
        <Skeleton className="h-16 w-full" />
      ) : tracks.length === 0 ? (
        <Text variant="muted">
          No tracks yet. Add one to promote builds without rebuilding them.
        </Text>
      ) : (
        <RowList>
          {tracks.map((environment) => (
            <TrackSetupRow
              key={environment.id}
              canManage={canManage}
              environment={environment}
              onDelete={() => setDeleting(environment)}
              onEdit={() => setEditing(environment)}
            />
          ))}
        </RowList>
      )}

      <EnvironmentDialog
        open={createOpen}
        onOpenChange={setCreateOpen}
        projectId={projectId}
        store={store}
      />
      <EnvironmentDialog
        open={editing !== undefined}
        onOpenChange={(open) => {
          if (!open) setEditing(undefined)
        }}
        environment={editing}
        projectId={projectId}
        store={store}
      />
      <ConfirmDialog
        open={deleting !== undefined}
        onOpenChange={(open) => {
          if (!open) setDeleting(undefined)
        }}
        title={`Delete ${deleting?.name ?? 'track'}?`}
        description="Its releases stay, and promotions waiting for approval are rejected. Nothing changes in the store."
        confirmLabel="Delete"
        confirmVariant="destructive"
        isPending={deleteMutation.isPending}
        onConfirm={handleDelete}
      />
    </section>
  )
}
