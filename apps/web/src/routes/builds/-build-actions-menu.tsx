import { lazy, Suspense, useState } from 'react'
import { Link, useNavigate } from '@tanstack/react-router'
import { toast } from 'sonner'

import type { Build } from '@oore/client/models'
import { DataTableRowActionsTrigger } from '@/components/data-table'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
} from '@/components/ui/dropdown-menu'
import { latestAppPerPlatform } from '@/lib/artifact-install'
import {
  isTerminalStatus,
  useArtifacts,
  useBuildApprovalDecision,
  useCancelBuild,
  useRerunBuild,
} from '@/hooks/use-builds'
import {
  hasProjectPermission,
  useHasPermissions,
} from '@/hooks/use-permissions'
import { useProject } from '@/hooks/use-projects'
import { useCopyInstallLink } from '@/hooks/use-copy-install-link'

const CancelBuildDialog = lazy(
  () => import('@/components/build-details/cancel-build-dialog'),
)
const RejectBuildDialog = lazy(
  () => import('@/components/build-details/reject-build-dialog'),
)

export default function BuildActionsMenu({ build }: { build: Build }) {
  const [open, setOpen] = useState(false)
  const [confirm, setConfirm] = useState<'cancel' | 'reject' | null>(null)
  const cancelMutation = useCancelBuild()
  const rejectMutation = useBuildApprovalDecision('reject')

  return (
    <>
      <DropdownMenu open={open} onOpenChange={setOpen}>
        <DataTableRowActionsTrigger
          label={`Actions for build #${build.build_number}`}
        />
        <DropdownMenuContent align="end" className="w-48">
          {open ? (
            <BuildActionsMenuItems
              build={build}
              onCancel={() => setConfirm('cancel')}
              onReject={() => setConfirm('reject')}
            />
          ) : null}
        </DropdownMenuContent>
      </DropdownMenu>

      {confirm === 'cancel' ? (
        <Suspense fallback={null}>
          <CancelBuildDialog
            buildNumber={build.build_number}
            isPending={cancelMutation.isPending}
            onCancel={() =>
              cancelMutation.mutate(build.id, {
                onSuccess: () => {
                  toast.success(`Build #${build.build_number} canceled`)
                  setConfirm(null)
                },
                onError: (err) =>
                  toast.error(`Failed to cancel: ${err.message}`),
              })
            }
            onOpenChange={(next) => setConfirm(next ? 'cancel' : null)}
            open
          />
        </Suspense>
      ) : null}
      {confirm === 'reject' ? (
        <Suspense fallback={null}>
          <RejectBuildDialog
            buildNumber={build.build_number}
            isPending={rejectMutation.isPending}
            onReject={(reason) =>
              rejectMutation.mutate(
                { buildId: build.id, reason },
                {
                  onSuccess: () => {
                    toast.success(`Build #${build.build_number} rejected`)
                    setConfirm(null)
                  },
                  onError: (err) =>
                    toast.error(`Failed to reject: ${err.message}`),
                },
              )
            }
            onOpenChange={(next) => setConfirm(next ? 'reject' : null)}
            open
          />
        </Suspense>
      ) : null}
    </>
  )
}

function InstallLinkItems({ build }: { build: Build }) {
  const copyInstallLink = useCopyInstallLink()
  const artifactsQuery = useArtifacts(build.id)
  const apps = latestAppPerPlatform(artifactsQuery.data?.artifacts ?? [])

  if (artifactsQuery.isLoading) {
    return <DropdownMenuItem disabled>Checking install links…</DropdownMenuItem>
  }
  if (apps.length === 0) return null

  return (
    <>
      {apps.map(({ artifact, label }) => (
        <DropdownMenuItem
          key={artifact.id}
          onClick={() => copyInstallLink(build.id, artifact, label)}
        >
          {apps.length === 1
            ? 'Copy install link'
            : `Copy ${label} install link`}
        </DropdownMenuItem>
      ))}
    </>
  )
}

// Mounted only while the menu is open, so a page of rows does not fetch each
// project's role up front.
function BuildActionsMenuItems({
  build,
  onCancel,
  onReject,
}: {
  build: Build
  onCancel: () => void
  onReject: () => void
}) {
  const navigate = useNavigate()
  const [canTriggerGlobally, canCancelGlobally] = useHasPermissions([
    'builds:write',
    'builds:cancel',
  ])
  const projectQuery = useProject(build.project_id)
  const projectRole = projectQuery.data?.project.current_user_role
  const approveMutation = useBuildApprovalDecision('approve')
  const rerunMutation = useRerunBuild()

  const isTerminal = isTerminalStatus(build.status)
  const awaitingApproval = build.approval?.state === 'pending'
  const canDecideApproval =
    awaitingApproval && hasProjectPermission(projectRole, 'builds:approve')
  const canRerun =
    isTerminal &&
    canTriggerGlobally &&
    hasProjectPermission(projectRole, 'builds:write')
  // A maintainer rejects rather than cancels a build waiting for approval.
  const canCancel =
    !isTerminal &&
    !canDecideApproval &&
    canCancelGlobally &&
    hasProjectPermission(projectRole, 'builds:cancel')
  const hasBuildActions = canDecideApproval || canRerun || canCancel

  return (
    <>
      <DropdownMenuGroup>
        <DropdownMenuLabel>Actions</DropdownMenuLabel>
        {projectQuery.isLoading ? (
          <DropdownMenuItem disabled>Checking permissions…</DropdownMenuItem>
        ) : null}
        {canDecideApproval ? (
          <>
            <DropdownMenuItem
              disabled={approveMutation.isPending}
              onClick={() =>
                approveMutation.mutate(
                  { buildId: build.id },
                  {
                    onSuccess: () =>
                      toast.success(`Build #${build.build_number} approved`),
                    onError: (err) =>
                      toast.error(`Failed to approve: ${err.message}`),
                  },
                )
              }
            >
              Approve
            </DropdownMenuItem>
            <DropdownMenuItem variant="destructive" onClick={onReject}>
              Reject…
            </DropdownMenuItem>
          </>
        ) : null}
        {canRerun ? (
          <DropdownMenuItem
            disabled={rerunMutation.isPending}
            onClick={() =>
              rerunMutation.mutate(build.id, {
                onSuccess: (result) => {
                  toast.success(
                    `Re-run queued as build #${result.build.build_number}`,
                  )
                  void navigate({
                    to: '/builds/$buildId',
                    params: { buildId: result.build.id },
                  })
                },
                onError: (err) =>
                  toast.error(`Failed to re-run: ${err.message}`),
              })
            }
          >
            Re-run
          </DropdownMenuItem>
        ) : null}
        {canCancel ? (
          <DropdownMenuItem variant="destructive" onClick={onCancel}>
            Cancel build…
          </DropdownMenuItem>
        ) : null}
      </DropdownMenuGroup>
      {hasBuildActions || projectQuery.isLoading ? (
        <DropdownMenuSeparator />
      ) : null}
      <DropdownMenuGroup>
        {isTerminal ? <InstallLinkItems build={build} /> : null}
        <DropdownMenuItem
          render={
            <Link
              to="/projects/$projectId/pipelines/$pipelineId"
              params={{
                projectId: build.project_id,
                pipelineId: build.pipeline_id,
              }}
            />
          }
        >
          Open pipeline
        </DropdownMenuItem>
        <DropdownMenuItem
          render={
            <Link
              to="/projects/$projectId"
              params={{ projectId: build.project_id }}
              search={{ tab: 'builds' }}
            />
          }
        >
          Project builds
        </DropdownMenuItem>
      </DropdownMenuGroup>
    </>
  )
}
