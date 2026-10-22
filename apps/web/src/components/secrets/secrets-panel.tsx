import { useState } from 'react'
import { Link } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  Add01Icon,
  LockPasswordIcon,
  MoreHorizontalCircle01Icon,
} from '@hugeicons/core-free-icons'

import { getApiErrorMessage } from '@/lib/api-client/api-error'
import { relativeTime } from '@/lib/format-utils'
import { toast } from '@/lib/toast'
import {
  useDeleteSecret,
  useInheritedSecrets,
  useSecrets,
  type SecretScope,
  type SecretSummary,
  type SecretTarget,
} from '@/hooks/use-secrets'
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
import { SecretValueDialog } from './secret-value-dialog'

const SCOPE_LABEL = {
  instance: 'Instance',
  project: 'Project',
  pipeline: 'Pipeline',
} as const satisfies Record<SecretScope, string>

const SCOPE_DESCRIPTION = {
  instance:
    'Available to every build on this instance. A project or pipeline secret with the same name takes precedence.',
  project:
    'Available to every pipeline in this project. A pipeline secret with the same name takes precedence; this one takes precedence over an instance secret.',
  pipeline:
    'Available only to this pipeline, and take precedence over project and instance secrets with the same name.',
} as const satisfies Record<SecretScope, string>

export function secretReference(name: string) {
  return `\${{ secrets.${name} }}`
}

function copyReference(name: string) {
  void navigator.clipboard.writeText(secretReference(name)).then(
    () => toast.success('Reference copied'),
    () => toast.error('Could not copy to the clipboard'),
  )
}

function SecretRow({
  canWrite,
  children,
  name,
  onDelete,
  onRotate,
  updatedAt,
}: {
  canWrite: boolean
  children?: React.ReactNode
  name: string
  onDelete?: () => void
  onRotate?: () => void
  updatedAt: number
}) {
  return (
    <Item size="sm" role="listitem">
      <ItemContent className="min-w-0">
        <ItemTitle className="font-mono">{name}</ItemTitle>
        <ItemDescription>Updated {relativeTime(updatedAt)}</ItemDescription>
      </ItemContent>
      <ItemActions>
        {children}
        <DropdownMenu>
          <DropdownMenuTrigger
            render={<Button variant="ghost" size="icon-xs" />}
          >
            <span className="sr-only">Actions for {name}</span>
            <HugeiconsIcon icon={MoreHorizontalCircle01Icon} />
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end" className="w-48">
            <DropdownMenuGroup>
              <DropdownMenuLabel>Actions</DropdownMenuLabel>
              <DropdownMenuItem onClick={() => copyReference(name)}>
                Copy reference
              </DropdownMenuItem>
              {canWrite && onRotate ? (
                <DropdownMenuItem onClick={onRotate}>
                  Replace value
                </DropdownMenuItem>
              ) : null}
            </DropdownMenuGroup>
            {canWrite && onDelete ? (
              <>
                <DropdownMenuSeparator />
                <DropdownMenuGroup>
                  <DropdownMenuItem variant="destructive" onClick={onDelete}>
                    Delete
                  </DropdownMenuItem>
                </DropdownMenuGroup>
              </>
            ) : null}
          </DropdownMenuContent>
        </DropdownMenu>
      </ItemActions>
    </Item>
  )
}

function RowList({ children }: { children: Array<React.ReactNode> }) {
  return (
    <SettingsSurface inset={false}>
      <ItemGroup className="gap-0">
        {children.flatMap((child, index) =>
          index === 0
            ? [child]
            : [
                <ItemSeparator key={`separator-${index}`} className="my-0" />,
                child,
              ],
        )}
      </ItemGroup>
    </SettingsSurface>
  )
}

/** Where an inherited secret is managed. */
function OwnerScopeLink({ target }: { target: SecretTarget }) {
  if (target.scope === 'pipeline') return null
  return (
    <Button
      variant="ghost"
      size="xs"
      nativeButton={false}
      render={
        target.scope === 'instance' ? (
          <Link to="/settings/secrets" />
        ) : (
          <Link
            to="/projects/$projectId"
            params={{ projectId: target.projectId }}
            search={{ tab: 'settings' }}
          />
        )
      }
    >
      Manage
    </Button>
  )
}

/**
 * Write-only secrets for one scope. `inheritedFrom` names the broader scopes
 * (narrowest first) this scope also sees, so the effect of a name clash shows.
 */
export function SecretsPanel({
  canWrite,
  createOpen: controlledCreateOpen,
  inheritedFrom = [],
  onCreateOpenChange,
  target,
  title = 'Secrets',
}: {
  canWrite: boolean
  /** Pass with `onCreateOpenChange` when the page header owns Add secret. */
  createOpen?: boolean
  inheritedFrom?: Array<SecretTarget>
  onCreateOpenChange?: (open: boolean) => void
  target: SecretTarget
  title?: string
}) {
  const secretsQuery = useSecrets(target)
  const inherited = useInheritedSecrets(inheritedFrom)
  const deleteMutation = useDeleteSecret(target)
  const [ownCreateOpen, setOwnCreateOpen] = useState(false)
  const headerOwnsCreate = onCreateOpenChange !== undefined
  const createOpen = controlledCreateOpen ?? ownCreateOpen
  const setCreateOpen = onCreateOpenChange ?? setOwnCreateOpen
  const [rotateName, setRotateName] = useState<string>()
  const [deleteName, setDeleteName] = useState<string>()
  const secrets = secretsQuery.data ?? []
  const ownNames = new Set(secrets.map((secret) => secret.name))
  const inheritedByName = new Map<string, SecretSummary>()
  // Narrowest inherited scope wins, matching the daemon's resolution order.
  for (const secret of inherited) {
    if (!inheritedByName.has(secret.name)) {
      inheritedByName.set(secret.name, secret)
    }
  }
  const visibleInherited = [...inheritedByName.values()]
  const inheritedScopes = new Set(
    visibleInherited.map((secret) => secret.scope),
  )
  const [onlyInheritedScope] = inheritedScopes
  const inheritedHeading =
    inheritedScopes.size === 1 && onlyInheritedScope
      ? `Inherited from ${SCOPE_LABEL[onlyInheritedScope].toLowerCase()}`
      : 'Inherited from broader scopes'

  function handleDelete() {
    if (!deleteName) return
    deleteMutation.mutate(deleteName, {
      onSuccess: () => {
        toast.success(`Deleted ${deleteName}`)
        setDeleteName(undefined)
      },
      onError: (err) => toast.error(getApiErrorMessage(err, {})),
    })
  }

  const fallback = deleteName ? inheritedByName.get(deleteName) : undefined

  return (
    <SettingsSection
      title={title}
      description={SCOPE_DESCRIPTION[target.scope]}
      actions={
        canWrite && !headerOwnsCreate ? (
          <Button size="sm" onClick={() => setCreateOpen(true)}>
            <HugeiconsIcon icon={Add01Icon} />
            Add secret
          </Button>
        ) : undefined
      }
    >
      {secretsQuery.error ? (
        <CollectionError
          title="Failed to load secrets"
          description={secretsQuery.error.message}
          onRetry={() => void secretsQuery.refetch()}
        />
      ) : secretsQuery.isLoading ? (
        <Skeleton className="h-16 w-full" />
      ) : secrets.length === 0 ? (
        <Empty className="border bg-card p-6">
          <EmptyHeader>
            <EmptyMedia variant="icon">
              <HugeiconsIcon icon={LockPasswordIcon} />
            </EmptyMedia>
            <EmptyTitle>
              No {SCOPE_LABEL[target.scope].toLowerCase()} secrets
            </EmptyTitle>
            <EmptyDescription>
              Store tokens and keys here instead of in env values, then
              reference them as{' '}
              <code className="font-mono">{secretReference('NAME')}</code>.
              Values are masked as *** in build logs.
            </EmptyDescription>
          </EmptyHeader>
          {canWrite ? (
            <EmptyContent>
              <Button
                size="sm"
                variant={headerOwnsCreate ? 'outline' : 'default'}
                onClick={() => setCreateOpen(true)}
              >
                Add secret
              </Button>
            </EmptyContent>
          ) : null}
        </Empty>
      ) : (
        <RowList>
          {secrets.map((secret) => {
            const shadowed = inheritedByName.get(secret.name)
            return (
              <SecretRow
                key={secret.name}
                canWrite={canWrite}
                name={secret.name}
                updatedAt={secret.updated_at}
                onRotate={() => setRotateName(secret.name)}
                onDelete={() => setDeleteName(secret.name)}
              >
                {shadowed ? (
                  <Badge variant="outline">
                    Overrides {SCOPE_LABEL[shadowed.scope].toLowerCase()}
                  </Badge>
                ) : null}
              </SecretRow>
            )
          })}
        </RowList>
      )}

      {visibleInherited.length > 0 ? (
        <div className="flex flex-col gap-3">
          <div className="flex flex-col gap-1">
            <h3 className="text-sm font-medium">{inheritedHeading}</h3>
            <p className="text-sm text-muted-foreground">
              Builds here can use these too. Manage them where they are stored;
              a secret above with the same name overrides them.
            </p>
          </div>
          <RowList>
            {visibleInherited.map((secret) => {
              const overridden = ownNames.has(secret.name)
              const owner = inheritedFrom.find(
                (candidate) => candidate.scope === secret.scope,
              )
              return (
                <SecretRow
                  key={`${secret.scope}-${secret.name}`}
                  canWrite={false}
                  name={secret.name}
                  updatedAt={secret.updated_at}
                >
                  <Badge variant={overridden ? 'outline' : 'secondary'}>
                    {SCOPE_LABEL[secret.scope]}
                    {overridden ? ' · overridden' : ''}
                  </Badge>
                  {owner ? <OwnerScopeLink target={owner} /> : null}
                </SecretRow>
              )
            })}
          </RowList>
        </div>
      ) : null}

      <SecretValueDialog
        open={createOpen}
        onOpenChange={setCreateOpen}
        target={target}
      />
      <SecretValueDialog
        open={rotateName !== undefined}
        onOpenChange={(open) => {
          if (!open) setRotateName(undefined)
        }}
        name={rotateName ?? ''}
        target={target}
      />
      <ConfirmDialog
        open={deleteName !== undefined}
        onOpenChange={(open) => {
          if (!open) setDeleteName(undefined)
        }}
        title={`Delete ${deleteName ?? 'secret'}?`}
        description={
          fallback
            ? `Builds will use the ${SCOPE_LABEL[fallback.scope].toLowerCase()} secret with this name instead.`
            : 'Builds that reference it will fail before any step runs until it is added again. This cannot be undone.'
        }
        confirmLabel="Delete"
        confirmVariant="destructive"
        isPending={deleteMutation.isPending}
        onConfirm={handleDelete}
      />
    </SettingsSection>
  )
}
