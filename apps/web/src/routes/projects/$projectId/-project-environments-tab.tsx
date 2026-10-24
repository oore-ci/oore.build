import { useState } from 'react'
import { Link } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  Add01Icon,
  ArrowRight02Icon,
  Layers01Icon,
  MoreHorizontalCircle01Icon,
} from '@hugeicons/core-free-icons'

import type {
  EnvironmentPromotion,
  ProjectEnvironment,
} from '@oore/client/models'
import { getApiErrorMessage } from '@/lib/api-client/api-error'
import {
  environmentTargetLabel,
  getPromotionStateVariant,
  PROMOTION_STATE_LABEL,
  promotionSourceLabel,
  rolloutPolicySummary,
} from '@/lib/environments'
import { relativeTime } from '@/lib/format-utils'
import { isLiveRelease, releaseVersionLabel } from '@/lib/releases'
import { toast } from '@/lib/toast'
import {
  useDecidePromotion,
  useDeleteEnvironment,
  useProjectEnvironments,
} from '@/hooks/use-environments'
import ConfirmDialog from '@/components/ConfirmDialog'
import { CollectionError } from '@/components/collection'
import { EnvironmentDialog } from '@/components/environments/environment-dialog'
import { PromoteDialog } from '@/components/environments/promote-dialog'
import { ReleaseRollout } from '@/components/releases/release-rollout'
import {
  SettingsSection,
  SettingsSurface,
} from '@/components/settings/settings-section'
import { Stack } from '@/components/layout/stack'
import { Text } from '@/components/layout/text'
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
import { TabsContent } from '@/components/ui/tabs'

const ENVIRONMENTS_DOCS_URL =
  'https://docs.oore.build/build/distribute/environments'

/**
 * A release in this environment that can still be promoted elsewhere: a live
 * Google Play release, or a TestFlight release testers have. A release on the
 * App Store itself is the end of the path.
 */
function promotableRelease(environment: ProjectEnvironment) {
  const release = environment.latest_release
  if (!release) return undefined
  if (release.store === 'app_store' && release.track === 'app_store') {
    return undefined
  }
  return ['rolling', 'paused', 'complete'].includes(release.state)
    ? release
    : undefined
}

function RowList({ children }: { children: Array<React.ReactNode> }) {
  return (
    <SettingsSurface inset={false}>
      <ItemGroup className="gap-0" role="list">
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

function EnvironmentRow({
  canManage,
  canPromote,
  environment,
  onDelete,
  onEdit,
  onPromote,
}: {
  canManage: boolean
  canPromote: boolean
  environment: ProjectEnvironment
  onDelete: () => void
  onEdit: () => void
  onPromote: () => void
}) {
  const release = environment.latest_release
  return (
    <Item role="listitem" className="flex-wrap gap-y-3">
      <ItemContent className="min-w-48 basis-56">
        <ItemTitle>
          {environment.name}
          <Badge variant="outline" className="font-mono text-[11px]">
            {environment.key}
          </Badge>
          {environment.approval_required ? (
            <Badge variant="secondary">Approval required</Badge>
          ) : null}
        </ItemTitle>
        <ItemDescription>
          {environmentTargetLabel(environment)} ·{' '}
          {rolloutPolicySummary(environment)}
        </ItemDescription>
      </ItemContent>
      <ItemContent className="min-w-48 basis-64">
        {release ? (
          <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
            <Stack className="min-w-0">
              <Link
                to="/releases/$releaseId"
                params={{ releaseId: release.id }}
                className="truncate text-sm font-medium underline-offset-4 hover:underline"
              >
                {releaseVersionLabel(release)}
              </Link>
              <Text variant="caption" render={<span />}>
                {release.build_number
                  ? `Build #${release.build_number} · `
                  : ''}
                {release.promoted_from_release_id ? 'Promoted' : 'Uploaded'}{' '}
                {relativeTime(release.created_at)}
              </Text>
            </Stack>
            {isLiveRelease(release) || release.state === 'complete' ? (
              <ReleaseRollout release={release} className="w-40" />
            ) : null}
          </div>
        ) : (
          <Text render={<span />}>No release yet</Text>
        )}
      </ItemContent>
      {/* A fixed width keeps the release column aligned across rows. */}
      <ItemActions className="justify-end sm:w-36">
        {canPromote ? (
          <Button size="sm" variant="outline" onClick={onPromote}>
            Promote
            <HugeiconsIcon icon={ArrowRight02Icon} data-icon="inline-end" />
          </Button>
        ) : null}
        {canManage ? (
          <DropdownMenu>
            <DropdownMenuTrigger
              render={<Button variant="ghost" size="icon-xs" />}
            >
              <span className="sr-only">Actions for {environment.name}</span>
              <HugeiconsIcon icon={MoreHorizontalCircle01Icon} />
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" className="w-48">
              <DropdownMenuGroup>
                <DropdownMenuLabel>Actions</DropdownMenuLabel>
                <DropdownMenuItem onClick={onEdit}>Edit</DropdownMenuItem>
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

function PromotionRow({
  onDecide,
  pending,
  promotion,
}: {
  onDecide: (approve: boolean) => void
  pending: boolean
  promotion: EnvironmentPromotion
}) {
  const build = promotion.build_number
    ? `Build #${promotion.build_number}`
    : 'Build'
  const version = promotion.version_name
    ? ` · ${promotion.version_name}${promotion.version_code == null ? '' : ` (${promotion.version_code})`}`
    : ''
  const who =
    promotion.state === 'pending' || !promotion.decided_by_email
      ? promotion.requested_by_email
        ? `Requested by ${promotion.requested_by_email}`
        : 'Requested'
      : `${promotion.state === 'rejected' ? 'Rejected' : 'Approved'} by ${promotion.decided_by_email}`
  const note =
    promotion.state === 'failed'
      ? promotion.error
      : (promotion.decision_reason ?? promotion.reason)
  return (
    <Item size="sm" role="listitem" className="flex-wrap gap-y-2">
      <ItemContent className="min-w-56">
        <ItemTitle>
          {build}
          {version}
          <span className="inline-flex items-center gap-1 font-normal text-muted-foreground">
            {promotionSourceLabel(promotion)}
            <HugeiconsIcon icon={ArrowRight02Icon} size={14} />
            {promotion.environment_name}
          </span>
        </ItemTitle>
        <ItemDescription>
          {who} · {relativeTime(promotion.decided_at ?? promotion.created_at)}
          {note ? ` · ${note}` : ''}
        </ItemDescription>
      </ItemContent>
      <ItemActions>
        {promotion.can_decide ? (
          <>
            <Button
              size="sm"
              variant="outline"
              disabled={pending}
              onClick={() => onDecide(false)}
            >
              Reject
            </Button>
            <Button size="sm" disabled={pending} onClick={() => onDecide(true)}>
              {pending ? <Spinner data-icon="inline-start" /> : null}
              Approve
            </Button>
          </>
        ) : (
          <Badge variant={getPromotionStateVariant(promotion.state)}>
            {PROMOTION_STATE_LABEL[promotion.state]}
          </Badge>
        )}
      </ItemActions>
    </Item>
  )
}

export function ProjectEnvironmentsTab({
  canManage,
  canOperate,
  projectId,
}: {
  /** Maintainers add environments, promote straight in and approve. */
  canManage: boolean
  /** Developers may ask for promotions into environments that need approval. */
  canOperate: boolean
  projectId: string
}) {
  const environmentsQuery = useProjectEnvironments(projectId)
  const deleteMutation = useDeleteEnvironment(projectId)
  const decideMutation = useDecidePromotion(projectId)
  const [createOpen, setCreateOpen] = useState(false)
  const [editing, setEditing] = useState<ProjectEnvironment>()
  const [deleting, setDeleting] = useState<ProjectEnvironment>()
  const [promoting, setPromoting] = useState<ProjectEnvironment>()
  const environments = environmentsQuery.data?.environments ?? []
  const promotions = environmentsQuery.data?.promotions ?? []
  const pendingPromotions = promotions.filter(
    (promotion) => promotion.state === 'pending',
  )
  const recentPromotions = promotions.filter(
    (promotion) => promotion.state !== 'pending',
  )
  const promotingRelease = promoting ? promotableRelease(promoting) : undefined

  /** Promote is offered toward later environments on the path. */
  function canPromoteFrom(environment: ProjectEnvironment) {
    if (!promotableRelease(environment)) return false
    const index = environments.indexOf(environment)
    return environments
      .slice(index + 1)
      .some(
        (target) =>
          target.promotable &&
          target.store === environment.store &&
          (canManage || (canOperate && target.approval_required)),
      )
  }

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

  function decide(promotion: EnvironmentPromotion, approve: boolean) {
    decideMutation.mutate(
      { approve, promotionId: promotion.id },
      {
        onSuccess: (decided) =>
          toast.success(
            approve
              ? `Promoted build #${decided.build_number ?? ''} to ${decided.environment_name}`
              : `Rejected the promotion to ${decided.environment_name}`,
          ),
        onError: (err) => toast.error(getApiErrorMessage(err, {})),
      },
    )
  }

  return (
    <TabsContent value="environments">
      <Stack gap={8} className="pt-2">
        <SettingsSection
          title="Environments"
          hint="Promoting a build reuses the version the store already has; nothing is rebuilt."
          meta={
            environments.length > 0 ? (
              <Badge variant="secondary">{environments.length}</Badge>
            ) : undefined
          }
          actions={
            canManage && environments.length > 0 ? (
              <Button
                size="sm"
                variant="outline"
                onClick={() => setCreateOpen(true)}
              >
                <HugeiconsIcon icon={Add01Icon} />
                Add environment
              </Button>
            ) : undefined
          }
        >
          {environmentsQuery.error ? (
            <CollectionError
              title="Failed to load environments"
              description={environmentsQuery.error.message}
              onRetry={() => void environmentsQuery.refetch()}
            />
          ) : environmentsQuery.isLoading ? (
            <Skeleton className="h-40 w-full" />
          ) : environments.length === 0 ? (
            <Empty className="border bg-card p-6">
              <EmptyHeader>
                <EmptyMedia variant="icon">
                  <HugeiconsIcon icon={Layers01Icon} />
                </EmptyMedia>
                <EmptyTitle>No environments</EmptyTitle>
                <EmptyDescription>
                  Move a tested build from internal testing to production
                  without rebuilding it, with approvals where you need them.{' '}
                  <a
                    href={ENVIRONMENTS_DOCS_URL}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="underline underline-offset-4"
                  >
                    Learn more
                  </a>
                </EmptyDescription>
              </EmptyHeader>
              {canManage ? (
                <EmptyContent>
                  <Button size="sm" onClick={() => setCreateOpen(true)}>
                    Add environment
                  </Button>
                </EmptyContent>
              ) : null}
            </Empty>
          ) : (
            <RowList>
              {environments.map((environment) => (
                <EnvironmentRow
                  key={environment.id}
                  canManage={canManage}
                  canPromote={canPromoteFrom(environment)}
                  environment={environment}
                  onDelete={() => setDeleting(environment)}
                  onEdit={() => setEditing(environment)}
                  onPromote={() => setPromoting(environment)}
                />
              ))}
            </RowList>
          )}
        </SettingsSection>

        {pendingPromotions.length > 0 ? (
          <SettingsSection
            title="Waiting for approval"
            meta={<Badge variant="warning">{pendingPromotions.length}</Badge>}
          >
            <RowList>
              {pendingPromotions.map((promotion) => (
                <PromotionRow
                  key={promotion.id}
                  promotion={promotion}
                  pending={
                    decideMutation.isPending &&
                    decideMutation.variables.promotionId === promotion.id
                  }
                  onDecide={(approve) => decide(promotion, approve)}
                />
              ))}
            </RowList>
          </SettingsSection>
        ) : null}

        {recentPromotions.length > 0 ? (
          <SettingsSection title="Recent promotions">
            <RowList>
              {recentPromotions.map((promotion) => (
                <PromotionRow
                  key={promotion.id}
                  promotion={promotion}
                  pending={false}
                  onDecide={() => undefined}
                />
              ))}
            </RowList>
          </SettingsSection>
        ) : null}
      </Stack>

      <EnvironmentDialog
        open={createOpen}
        onOpenChange={setCreateOpen}
        projectId={projectId}
      />
      <EnvironmentDialog
        open={editing !== undefined}
        onOpenChange={(open) => {
          if (!open) setEditing(undefined)
        }}
        environment={editing}
        projectId={projectId}
      />
      {promoting && promotingRelease ? (
        <PromoteDialog
          open
          onOpenChange={(open) => {
            if (!open) setPromoting(undefined)
          }}
          canPromoteDirectly={canManage}
          environments={environments}
          projectId={projectId}
          release={promotingRelease}
          source={promoting}
        />
      ) : null}
      <ConfirmDialog
        open={deleting !== undefined}
        onOpenChange={(open) => {
          if (!open) setDeleting(undefined)
        }}
        title={`Delete ${deleting?.name ?? 'environment'}?`}
        description="Its releases stay, without an environment, and promotions waiting for approval are rejected. Nothing changes in the store."
        confirmLabel="Delete"
        confirmVariant="destructive"
        isPending={deleteMutation.isPending}
        onConfirm={handleDelete}
      />
    </TabsContent>
  )
}
