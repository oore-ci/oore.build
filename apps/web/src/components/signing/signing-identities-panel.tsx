import { useState } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  Add01Icon,
  Alert02Icon,
  Key01Icon,
  MoreHorizontalCircle01Icon,
} from '@hugeicons/core-free-icons'
import { isOoreApiError } from '@oore/client/client'

import type { SigningIdentity } from '@oore/client/models'
import {
  useDeleteSigningIdentity,
  useSigningIdentities,
} from '@/hooks/use-signing-identities'
import { getApiErrorMessage } from '@/lib/api-client/api-error'
import {
  SIGNING_IDENTITY_KIND_LABELS,
  expiryState,
  signingIdentityExpiry,
  signingIdentitySummary,
  signingIdentityUsage,
} from '@/lib/signing-identities'
import { toast } from '@/lib/toast'
import { CollectionError } from '@/components/collection'
import { RelativeTime } from '@/components/relative-time'
import {
  SettingsSection,
  SettingsSurface,
} from '@/components/settings/settings-section'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import {
  AlertDialog,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
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
import { Spinner } from '@/components/ui/spinner'
import { Text } from '@/components/layout/text'
import { SigningIdentityDialog } from './signing-identity-dialog'

const SIGNING_DOCS_URL =
  'https://docs.oore.build/reference/config/oore-yaml-v2#signing'

function ExpiryBadge({ identity }: { identity: SigningIdentity }) {
  const expiresAt = signingIdentityExpiry(identity)
  if (expiresAt === undefined) return null
  const state = expiryState(expiresAt)
  if (state === 'ok') return null
  return (
    <Badge variant={state === 'expired' ? 'destructive' : 'outline'}>
      {state === 'expired' ? 'Expired' : 'Expires soon'}
    </Badge>
  )
}

function IdentityRow({
  canWrite,
  identity,
  onDelete,
  onEdit,
}: {
  canWrite: boolean
  identity: SigningIdentity
  onDelete: () => void
  onEdit: () => void
}) {
  return (
    <Item size="sm" role="listitem">
      <ItemContent className="min-w-0">
        <ItemTitle>
          <span className="font-mono">{identity.name}</span>
          <Badge variant="outline">
            {SIGNING_IDENTITY_KIND_LABELS[identity.kind]}
          </Badge>
          <ExpiryBadge identity={identity} />
        </ItemTitle>
        <ItemDescription className="truncate">
          {signingIdentitySummary(identity) || 'No details'} · Updated{' '}
          <RelativeTime at={identity.updated_at} />
        </ItemDescription>
        <ItemDescription className="truncate">
          {signingIdentityUsage(identity)}
        </ItemDescription>
      </ItemContent>
      <ItemActions>
        {canWrite ? (
          <DropdownMenu>
            <DropdownMenuTrigger
              render={<Button variant="ghost" size="icon-xs" />}
            >
              <span className="sr-only">Actions for {identity.name}</span>
              <HugeiconsIcon icon={MoreHorizontalCircle01Icon} />
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" className="w-48">
              <DropdownMenuGroup>
                <DropdownMenuLabel>Actions</DropdownMenuLabel>
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

/**
 * Confirms a delete and keeps the daemon's refusal on screen: an identity
 * that workflow settings pick cannot be deleted, and the message names them.
 */
function DeleteIdentityDialog({
  identity,
  onOpenChange,
  projectId,
}: {
  identity?: SigningIdentity
  onOpenChange: (open: boolean) => void
  projectId: string
}) {
  const deleteMutation = useDeleteSigningIdentity(projectId)
  const [refusal, setRefusal] = useState<string>()
  const [shown, setShown] = useState(identity)
  if (identity && identity !== shown) {
    setShown(identity)
    setRefusal(undefined)
  }

  function close(next: boolean) {
    if (!next && deleteMutation.isPending) return
    onOpenChange(next)
  }

  function handleDelete() {
    if (!identity) return
    deleteMutation.mutate(identity.id, {
      onSuccess: () => {
        toast.success(`Deleted ${identity.name}`)
        onOpenChange(false)
      },
      onError: (err) => {
        if (isOoreApiError(err) && err.code === 'identity_in_use') {
          setRefusal(err.message)
          return
        }
        toast.error(getApiErrorMessage(err, {}))
      },
    })
  }

  const uses = shown?.used_by ?? []
  return (
    <AlertDialog open={identity !== undefined} onOpenChange={close}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>
            Delete {shown?.name ?? 'identity'}?
          </AlertDialogTitle>
          <AlertDialogDescription>
            {uses.length > 0
              ? `${signingIdentityUsage(shown!)}. Pick another identity in those workflow settings first.`
              : 'Builds of files that name it under signing are refused until the file names another. This cannot be undone.'}
          </AlertDialogDescription>
        </AlertDialogHeader>
        {refusal ? (
          <Alert variant="destructive">
            <HugeiconsIcon icon={Alert02Icon} size={16} />
            <AlertTitle>Still in use</AlertTitle>
            <AlertDescription>{refusal}</AlertDescription>
          </Alert>
        ) : null}
        <AlertDialogFooter>
          <Button
            variant="outline"
            onClick={() => close(false)}
            disabled={deleteMutation.isPending}
          >
            {refusal ? 'Close' : 'Cancel'}
          </Button>
          {refusal ? null : (
            <Button
              variant="destructive"
              onClick={handleDelete}
              disabled={deleteMutation.isPending}
            >
              {deleteMutation.isPending ? (
                <Spinner data-icon="inline-start" />
              ) : null}
              Delete
            </Button>
          )}
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}

/** Project settings > Signing: the keystores and certificates workflows sign with. */
export function SigningIdentitiesPanel({
  canWrite,
  projectId,
}: {
  canWrite: boolean
  projectId: string
}) {
  const identitiesQuery = useSigningIdentities(projectId)
  const [createOpen, setCreateOpen] = useState(false)
  const [editing, setEditing] = useState<SigningIdentity>()
  const [deleting, setDeleting] = useState<SigningIdentity>()
  const identities = identitiesQuery.data ?? []

  return (
    <SettingsSection
      id="signing"
      title="Signing"
      hint="Files name these under signing; key material never leaves Oore."
      meta={
        identities.length > 0 ? (
          <Badge variant="secondary">{identities.length}</Badge>
        ) : undefined
      }
      actions={
        canWrite && identities.length > 0 ? (
          <Button
            size="sm"
            variant="outline"
            onClick={() => setCreateOpen(true)}
          >
            <HugeiconsIcon icon={Add01Icon} />
            Add identity
          </Button>
        ) : undefined
      }
    >
      {identitiesQuery.error ? (
        <CollectionError
          title="Failed to load signing identities"
          description={identitiesQuery.error.message}
          onRetry={() => void identitiesQuery.refetch()}
        />
      ) : identitiesQuery.isLoading ? (
        <Skeleton className="h-16 w-full" />
      ) : identities.length === 0 ? (
        <Empty className="border bg-card p-6">
          <EmptyHeader>
            <EmptyMedia variant="icon">
              <HugeiconsIcon icon={Key01Icon} />
            </EmptyMedia>
            <EmptyTitle>No signing identities yet</EmptyTitle>
            <EmptyDescription>
              Add a keystore or distribution certificate once, then let each
              workflow sign with its own.{' '}
              <a
                href={SIGNING_DOCS_URL}
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
                Add identity
              </Button>
            </EmptyContent>
          ) : null}
        </Empty>
      ) : (
        <SettingsSurface inset={false}>
          <ItemGroup className="gap-0" role="list">
            {identities.flatMap((identity, index) => [
              ...(index > 0
                ? [
                    <ItemSeparator
                      key={`sep-${identity.id}`}
                      className="my-0"
                    />,
                  ]
                : []),
              <IdentityRow
                key={identity.id}
                identity={identity}
                canWrite={canWrite}
                onEdit={() => setEditing(identity)}
                onDelete={() => setDeleting(identity)}
              />,
            ])}
          </ItemGroup>
        </SettingsSurface>
      )}
      {identities.length > 0 ? (
        <Text variant="caption">
          A pipeline’s Workflows settings can pick a different identity for a
          workflow than its file names.{' '}
          <a
            href={SIGNING_DOCS_URL}
            target="_blank"
            rel="noopener noreferrer"
            className="underline underline-offset-4"
          >
            Learn more
          </a>
        </Text>
      ) : null}

      <SigningIdentityDialog
        open={createOpen}
        onOpenChange={setCreateOpen}
        projectId={projectId}
      />
      <SigningIdentityDialog
        open={editing !== undefined}
        onOpenChange={(open) => {
          if (!open) setEditing(undefined)
        }}
        existing={editing}
        projectId={projectId}
      />
      <DeleteIdentityDialog
        identity={deleting}
        onOpenChange={(open) => {
          if (!open) setDeleting(undefined)
        }}
        projectId={projectId}
      />
    </SettingsSection>
  )
}
