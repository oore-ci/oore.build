import { HugeiconsIcon } from '@hugeicons/react'
import { Delete02Icon } from '@hugeicons/core-free-icons'
import { useState } from 'react'
import { toast } from '@/lib/toast'

import { DataTableRowActionsTrigger } from '@/components/data-table'
import type {
  ProjectMember,
  ProjectMemberCandidate,
  ProjectRole,
} from '@oore/client/models'
import {
  useProjectMembers,
  useRemoveProjectMember,
  useUpdateProjectMember,
} from '@/hooks/use-projects'
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Avatar, AvatarFallback, AvatarImage } from '@/components/ui/avatar'
import { Badge } from '@/components/ui/badge'
import {
  Item,
  ItemActions,
  ItemContent,
  ItemDescription,
  ItemGroup,
  ItemMedia,
  ItemSeparator,
  ItemTitle,
} from '@/components/ui/item'
import {
  SettingsSection,
  SettingsSurface,
} from '@/components/settings/settings-section'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
} from '@/components/ui/dropdown-menu'
import { Skeleton } from '@/components/ui/skeleton'
import { Text } from '@/components/layout/text'
import {
  PROJECT_ROLE_LABELS,
  PROJECT_ROLE_OPTIONS,
  hasImplicitProjectAccess,
} from '@/lib/project-access'
import { AddProjectMembersDialog } from './-add-project-members-dialog'

interface RoleLabelMap {
  [role: string]: string
}

const INSTANCE_ROLE_LABELS: RoleLabelMap = {
  owner: 'Owner',
  admin: 'Admin',
  developer: 'Developer',
  qa_viewer: 'QA Viewer',
}

function initials(
  user: Pick<ProjectMemberCandidate, 'email' | 'display_name'>,
): string {
  const source = user.display_name?.trim() || user.email
  return source
    .split(/[\s@._-]+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((part) => part[0]?.toUpperCase())
    .join('')
}

function MemberAvatar({ member }: { member: ProjectMember }) {
  return (
    <Avatar size="sm">
      {member.user_avatar_url ? (
        <AvatarImage
          src={member.user_avatar_url}
          alt=""
          referrerPolicy="no-referrer"
        />
      ) : null}
      <AvatarFallback>
        {initials({
          email: member.user_email,
          display_name: member.user_display_name,
        })}
      </AvatarFallback>
    </Avatar>
  )
}

function MemberActions({
  instanceRole,
  member,
  onRemove,
  onRoleChange,
  pending,
}: {
  instanceRole: string | undefined
  member: ProjectMember
  onRemove: () => void
  onRoleChange: (role: ProjectRole) => void
  pending: boolean
}) {
  const isQaViewer = instanceRole === 'qa_viewer'
  if (hasImplicitProjectAccess(instanceRole)) return null
  const name = member.user_display_name ?? member.user_email

  return (
    <DropdownMenu>
      <DataTableRowActionsTrigger label={`Actions for ${name}`} />
      <DropdownMenuContent align="end" className="w-44">
        <DropdownMenuGroup>
          <DropdownMenuLabel>Actions</DropdownMenuLabel>
        </DropdownMenuGroup>
        {!isQaViewer ? (
          <>
            <DropdownMenuGroup>
              <DropdownMenuSub>
                <DropdownMenuSubTrigger>Change role</DropdownMenuSubTrigger>
                <DropdownMenuSubContent>
                  <DropdownMenuRadioGroup value={member.role}>
                    {PROJECT_ROLE_OPTIONS.map((role) => (
                      <DropdownMenuRadioItem
                        key={role}
                        value={role}
                        disabled={pending}
                        onClick={() => onRoleChange(role)}
                      >
                        {PROJECT_ROLE_LABELS[role]}
                      </DropdownMenuRadioItem>
                    ))}
                  </DropdownMenuRadioGroup>
                </DropdownMenuSubContent>
              </DropdownMenuSub>
            </DropdownMenuGroup>
            <DropdownMenuSeparator />
          </>
        ) : null}
        <DropdownMenuGroup>
          <DropdownMenuItem variant="destructive" onClick={onRemove}>
            <HugeiconsIcon icon={Delete02Icon} />
            Remove access
          </DropdownMenuItem>
        </DropdownMenuGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}

export function ProjectAccessCard({ projectId }: { projectId: string }) {
  const membersQuery = useProjectMembers(projectId)
  const updateMutation = useUpdateProjectMember(projectId)
  const removeMutation = useRemoveProjectMember(projectId)
  const [memberToRemove, setMemberToRemove] = useState<ProjectMember | null>(
    null,
  )
  const members = membersQuery.data?.members ?? []

  function updateRole(member: ProjectMember, role: ProjectRole) {
    if (role === member.role) return
    updateMutation.mutate(
      { userId: member.user_id, data: { role } },
      {
        onSuccess: () =>
          toast.success(`Access updated for ${member.user_email}`),
        onError: (error) => toast.error(error.message),
      },
    )
  }

  function removeMember() {
    if (!memberToRemove) return
    removeMutation.mutate(memberToRemove.user_id, {
      onSuccess: () => {
        toast.success(`${memberToRemove.user_email} removed from this project`)
        setMemberToRemove(null)
      },
      onError: (error) => toast.error(error.message),
    })
  }

  const isLoading = membersQuery.isLoading
  const error = membersQuery.error

  return (
    <>
      <SettingsSection
        title="Access"
        actions={<AddProjectMembersDialog projectId={projectId} />}
      >
        <SettingsSurface inset={false}>
          {isLoading ? (
            <div className="space-y-3 p-4 sm:p-5">
              <Skeleton className="h-16 w-full" />
              <Skeleton className="h-16 w-full" />
            </div>
          ) : error ? (
            <Alert variant="destructive" className="m-4 w-auto sm:m-5">
              <AlertDescription>
                Failed to load project access: {error.message}
              </AlertDescription>
            </Alert>
          ) : members.length === 0 ? (
            <Text variant="muted" className="p-4 sm:p-5">
              No explicit project members yet. Owners and admins can always open
              this project.
            </Text>
          ) : (
            <ItemGroup className="gap-0" role="list">
              {members.flatMap((member, index) => {
                const implicit = hasImplicitProjectAccess(member.user_role)
                const projectRole =
                  PROJECT_ROLE_LABELS[
                    member.user_role === 'qa_viewer' ? 'viewer' : member.role
                  ]
                const row = (
                  <Item key={member.id} size="sm" role="listitem">
                    <ItemMedia>
                      <MemberAvatar member={member} />
                    </ItemMedia>
                    <ItemContent className="min-w-0">
                      <ItemTitle className="max-w-full">
                        <span className="truncate">
                          {member.user_display_name ?? member.user_email}
                        </span>
                        <Badge variant="outline">
                          {INSTANCE_ROLE_LABELS[member.user_role] ?? 'Unknown'}
                        </Badge>
                      </ItemTitle>
                      <ItemDescription className="wrap-break-word">
                        {member.user_display_name
                          ? `${member.user_email} · `
                          : ''}
                        {implicit
                          ? `Implicit ${projectRole.toLowerCase()} access from their instance role`
                          : `${projectRole} on this project`}
                      </ItemDescription>
                    </ItemContent>
                    <ItemActions>
                      <MemberActions
                        member={member}
                        instanceRole={member.user_role}
                        pending={updateMutation.isPending}
                        onRoleChange={(role) => updateRole(member, role)}
                        onRemove={() => setMemberToRemove(member)}
                      />
                    </ItemActions>
                  </Item>
                )
                return index === 0
                  ? [row]
                  : [
                      <ItemSeparator
                        key={`separator-${member.id}`}
                        className="my-0"
                      />,
                      row,
                    ]
              })}
            </ItemGroup>
          )}
        </SettingsSurface>
      </SettingsSection>

      <AlertDialog
        open={memberToRemove !== null}
        onOpenChange={(open) => {
          if (!open) setMemberToRemove(null)
        }}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Remove project access?</AlertDialogTitle>
            <AlertDialogDescription>
              {memberToRemove?.user_email} will no longer see this project’s
              builds, logs, or artifacts.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              variant="destructive"
              onClick={removeMember}
              disabled={removeMutation.isPending}
            >
              {removeMutation.isPending ? 'Removing...' : 'Remove access'}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  )
}
