import { useState } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  Add01Icon,
  AppStoreIcon,
  MoreHorizontalCircle01Icon,
} from '@hugeicons/core-free-icons'

import { getApiErrorMessage } from '@/lib/api-client/api-error'
import { relativeTime } from '@/lib/format-utils'
import { toast } from '@/lib/toast'
import {
  useAppStoreConnectKeys,
  useDeleteAppStoreConnectKey,
  type AppStoreConnectKeySummary,
} from '@/hooks/use-app-store-connect-keys'
import ConfirmDialog from '@/components/ConfirmDialog'
import { CollectionError } from '@/components/collection'
import {
  SettingsSection,
  SettingsSurface,
} from '@/components/settings/settings-section'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from '@/components/ui/empty'
import {
  Item,
  ItemActions,
  ItemContent,
  ItemDescription,
  ItemGroup,
  ItemSeparator,
  ItemTitle,
} from '@/components/ui/item'
import { Skeleton } from '@/components/ui/skeleton'
import { AppStoreConnectKeyDialog } from './app-store-connect-key-dialog'

const APP_STORE_CONNECT_DOCS_URL =
  'https://docs.oore.build/build/sign/ios/app-store-connect#2-share-the-key-across-pipelines'

export function pipelineUsage(count: number) {
  if (count === 0) return 'Not used yet'
  return count === 1 ? 'Used by 1 pipeline' : `Used by ${count} pipelines`
}

function KeyRow({
  canWrite,
  appKey,
  onDelete,
  onEdit,
}: {
  canWrite: boolean
  appKey: AppStoreConnectKeySummary
  onDelete: () => void
  onEdit: () => void
}) {
  return (
    <Item size="sm" role="listitem">
      <ItemContent className="min-w-0">
        <ItemTitle>{appKey.name}</ItemTitle>
        <ItemDescription className="truncate">
          <span className="font-mono">{appKey.key_id}</span> · Updated{' '}
          {relativeTime(appKey.updated_at)}
        </ItemDescription>
      </ItemContent>
      <ItemActions>
        <Badge variant={appKey.pipeline_count > 0 ? 'secondary' : 'outline'}>
          {pipelineUsage(appKey.pipeline_count)}
        </Badge>
        {canWrite ? (
          <DropdownMenu>
            <DropdownMenuTrigger
              render={<Button variant="ghost" size="icon-xs" />}
            >
              <span className="sr-only">Actions for {appKey.name}</span>
              <HugeiconsIcon icon={MoreHorizontalCircle01Icon} />
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" className="w-48">
              <DropdownMenuGroup>
                <DropdownMenuItem onClick={onEdit}>
                  Rename or replace
                </DropdownMenuItem>
              </DropdownMenuGroup>
              <DropdownMenuSeparator />
              <DropdownMenuGroup>
                <DropdownMenuItem variant="destructive" onClick={onDelete}>
                  Delete
                </DropdownMenuItem>
              </DropdownMenuGroup>
            </DropdownMenuContent>
          </DropdownMenu>
        ) : null}
      </ItemActions>
    </Item>
  )
}

export function AppStoreConnectKeysPanel({ canWrite }: { canWrite: boolean }) {
  const keysQuery = useAppStoreConnectKeys()
  const deleteMutation = useDeleteAppStoreConnectKey()
  const [createOpen, setCreateOpen] = useState(false)
  const [editing, setEditing] = useState<AppStoreConnectKeySummary>()
  const [deleting, setDeleting] = useState<AppStoreConnectKeySummary>()
  const keys = keysQuery.data ?? []

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
    <SettingsSection
      title="App Store Connect API keys"
      actions={
        canWrite && keys.length > 0 ? (
          <Button
            size="sm"
            variant="outline"
            onClick={() => setCreateOpen(true)}
          >
            <HugeiconsIcon icon={Add01Icon} />
            Add key
          </Button>
        ) : undefined
      }
    >
      {keysQuery.error ? (
        <CollectionError
          title="Failed to load App Store Connect keys"
          description={keysQuery.error.message}
          onRetry={() => void keysQuery.refetch()}
        />
      ) : keysQuery.isLoading ? (
        <Skeleton className="h-16 w-full" />
      ) : keys.length === 0 ? (
        <Empty className="border bg-card p-6">
          <EmptyHeader>
            <EmptyMedia variant="icon">
              <HugeiconsIcon icon={AppStoreIcon} />
            </EmptyMedia>
            <EmptyTitle>No shared keys yet</EmptyTitle>
            <EmptyDescription>
              Add your Apple team key once and pick it in each pipeline&apos;s
              iOS signing.{' '}
              <a
                href={APP_STORE_CONNECT_DOCS_URL}
                target="_blank"
                rel="noopener noreferrer"
                className="underline underline-offset-4"
              >
                Learn more
              </a>
            </EmptyDescription>
          </EmptyHeader>
          {canWrite ? (
            <EmptyContent>
              <Button size="sm" onClick={() => setCreateOpen(true)}>
                Add key
              </Button>
            </EmptyContent>
          ) : null}
        </Empty>
      ) : (
        <SettingsSurface inset={false}>
          <ItemGroup className="gap-0">
            {keys.flatMap((appKey, index) => [
              ...(index > 0
                ? [<ItemSeparator key={`sep-${appKey.id}`} className="my-0" />]
                : []),
              <KeyRow
                key={appKey.id}
                appKey={appKey}
                canWrite={canWrite}
                onEdit={() => setEditing(appKey)}
                onDelete={() => setDeleting(appKey)}
              />,
            ])}
          </ItemGroup>
        </SettingsSurface>
      )}
      {keys.length > 0 ? (
        <p className="text-xs text-muted-foreground">
          Pipelines pick a shared key in iOS signing, or upload their own.{' '}
          <a
            href={APP_STORE_CONNECT_DOCS_URL}
            target="_blank"
            rel="noopener noreferrer"
            className="underline underline-offset-4"
          >
            Learn more
          </a>
        </p>
      ) : null}

      <AppStoreConnectKeyDialog
        open={createOpen}
        onOpenChange={setCreateOpen}
      />
      <AppStoreConnectKeyDialog
        open={editing !== undefined}
        onOpenChange={(open) => {
          if (!open) setEditing(undefined)
        }}
        existing={editing}
      />
      <ConfirmDialog
        open={deleting !== undefined}
        onOpenChange={(open) => {
          if (!open) setDeleting(undefined)
        }}
        title={`Delete ${deleting?.name ?? 'key'}?`}
        description={
          deleting && deleting.pipeline_count > 0
            ? `${pipelineUsage(deleting.pipeline_count)}. Switch them to another key in iOS signing first; Oore will not delete a key that is in use.`
            : 'Pipelines can no longer pick this key. This cannot be undone.'
        }
        confirmLabel="Delete"
        confirmVariant="destructive"
        isPending={deleteMutation.isPending}
        onConfirm={handleDelete}
      />
    </SettingsSection>
  )
}
