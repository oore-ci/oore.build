import { zodResolver } from '@hookform/resolvers/zod'
import {
  Add01Icon,
  Delete02Icon,
  MoreHorizontalCircle01Icon,
} from '@hugeicons/core-free-icons'
import { HugeiconsIcon } from '@hugeicons/react'
import { useMemo, useState } from 'react'
import { useForm } from 'react-hook-form'
import { toast } from 'sonner'
import * as z from 'zod'

import type {
  ProjectMember,
  ProjectMemberCandidate,
  ProjectRole,
} from '@/lib/types'
import {
  useAddProjectMember,
  useProjectMemberCandidates,
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
import { Button } from '@/components/ui/button'
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/components/ui/card'
import {
  Combobox,
  ComboboxContent,
  ComboboxEmpty,
  ComboboxInput,
  ComboboxItem,
  ComboboxList,
} from '@/components/ui/combobox'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from '@/components/ui/dialog'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import {
  Form,
  FormControl,
  FormDescription,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
} from '@/components/ui/form'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Skeleton } from '@/components/ui/skeleton'
import { Spinner } from '@/components/ui/spinner'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'

const accessSchema = z.object({
  user_id: z.string().min(1, 'Select a user.'),
  role: z.enum(['maintainer', 'developer', 'viewer']),
})

type AccessForm = z.infer<typeof accessSchema>

const PROJECT_ROLE_LABELS: Record<ProjectRole, string> = {
  maintainer: 'Maintainer',
  developer: 'Developer',
  viewer: 'Viewer',
}

const PROJECT_ROLE_OPTIONS: Array<ProjectRole> = [
  'maintainer',
  'developer',
  'viewer',
]

const INSTANCE_ROLE_LABELS: Record<string, string> = {
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

function MemberIdentity({ member }: { member: ProjectMember }) {
  return (
    <div className="flex min-w-0 items-center gap-3">
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
      <div className="min-w-0">
        <p className="truncate font-medium">
          {member.user_display_name ?? member.user_email}
        </p>
        {member.user_display_name ? (
          <p className="truncate text-xs text-muted-foreground">
            {member.user_email}
          </p>
        ) : null}
      </div>
    </div>
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
  const hasImplicitAccess = instanceRole === 'owner' || instanceRole === 'admin'
  const isQaViewer = instanceRole === 'qa_viewer'
  if (hasImplicitAccess) return null

  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        render={
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label={`Manage access for ${member.user_email}`}
          />
        }
      >
        <HugeiconsIcon icon={MoreHorizontalCircle01Icon} />
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        {!isQaViewer ? (
          <>
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
            <DropdownMenuSeparator />
          </>
        ) : null}
        <DropdownMenuItem variant="destructive" onClick={onRemove}>
          <HugeiconsIcon icon={Delete02Icon} />
          Remove access
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}

export function ProjectAccessCard({ projectId }: { projectId: string }) {
  const membersQuery = useProjectMembers(projectId)
  const candidatesQuery = useProjectMemberCandidates(projectId)
  const addMutation = useAddProjectMember(projectId)
  const updateMutation = useUpdateProjectMember(projectId)
  const removeMutation = useRemoveProjectMember(projectId)
  const [addOpen, setAddOpen] = useState(false)
  const [memberToRemove, setMemberToRemove] = useState<ProjectMember | null>(
    null,
  )
  const form = useForm<AccessForm>({
    resolver: zodResolver(accessSchema),
    defaultValues: { user_id: '', role: 'viewer' },
  })

  const candidates = useMemo(
    () => candidatesQuery.data?.candidates ?? [],
    [candidatesQuery.data],
  )
  const members = useMemo(
    () => membersQuery.data?.members ?? [],
    [membersQuery.data],
  )
  const candidatesById = useMemo(
    () => new Map(candidates.map((candidate) => [candidate.id, candidate])),
    [candidates],
  )
  const selectedUser = candidatesById.get(form.watch('user_id'))
  const availableRoles =
    selectedUser?.role === 'qa_viewer'
      ? (['viewer'] as Array<ProjectRole>)
      : PROJECT_ROLE_OPTIONS

  function setDialogOpen(open: boolean) {
    setAddOpen(open)
    if (!open) form.reset({ user_id: '', role: 'viewer' })
  }

  function addMember(values: AccessForm) {
    const user = candidatesById.get(values.user_id)
    const role: ProjectRole =
      user?.role === 'qa_viewer' ? 'viewer' : values.role
    addMutation.mutate(
      { user_id: values.user_id, role },
      {
        onSuccess: () => {
          toast.success(`${user?.email ?? 'User'} added to this project`)
          setDialogOpen(false)
        },
        onError: (error) => toast.error(error.message),
      },
    )
  }

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

  const isLoading = membersQuery.isLoading || candidatesQuery.isLoading
  const error = membersQuery.error ?? candidatesQuery.error

  return (
    <>
      <Card>
        <CardHeader className="flex-row items-start justify-between gap-4">
          <div className="space-y-1.5">
            <CardTitle className="text-sm font-medium uppercase tracking-wider text-muted-foreground">
              Project access
            </CardTitle>
            <CardDescription>
              Grant developers or QA viewers access to this project.
            </CardDescription>
          </div>
          <Dialog open={addOpen} onOpenChange={setDialogOpen}>
            <DialogTrigger
              render={
                <Button
                  size="icon-sm"
                  aria-label="Add project member"
                  title="Add project member"
                />
              }
            >
              <HugeiconsIcon icon={Add01Icon} aria-hidden />
            </DialogTrigger>
            <DialogContent>
              <DialogHeader>
                <DialogTitle>Add project member</DialogTitle>
                <DialogDescription>
                  Choose an eligible user and their project role.
                </DialogDescription>
              </DialogHeader>
              <Form {...form}>
                <form
                  className="space-y-4"
                  onSubmit={form.handleSubmit(addMember)}
                >
                  <FormField
                    control={form.control}
                    name="user_id"
                    render={({ field }) => (
                      <FormItem>
                        <FormLabel>User</FormLabel>
                        <Combobox
                          items={candidates}
                          value={candidatesById.get(field.value) ?? null}
                          onValueChange={(user) => {
                            field.onChange(user?.id ?? '')
                            if (user?.role === 'qa_viewer') {
                              form.setValue('role', 'viewer')
                            }
                          }}
                          itemToStringLabel={(user) =>
                            user.display_name
                              ? `${user.display_name} (${user.email})`
                              : user.email
                          }
                        >
                          <FormControl>
                            <ComboboxInput
                              className="w-full"
                              placeholder="Search eligible users..."
                            />
                          </FormControl>
                          <ComboboxContent>
                            <ComboboxEmpty>
                              No eligible users found.
                            </ComboboxEmpty>
                            <ComboboxList>
                              {(user) => (
                                <ComboboxItem key={user.id} value={user}>
                                  <span className="min-w-0 flex-1 truncate">
                                    {user.email}
                                  </span>
                                  <span className="text-xs text-muted-foreground">
                                    {INSTANCE_ROLE_LABELS[user.role]}
                                    {user.status === 'invited'
                                      ? ' · Invited'
                                      : ''}
                                  </span>
                                </ComboboxItem>
                              )}
                            </ComboboxList>
                          </ComboboxContent>
                        </Combobox>
                        <FormMessage />
                      </FormItem>
                    )}
                  />
                  <FormField
                    control={form.control}
                    name="role"
                    render={({ field }) => (
                      <FormItem>
                        <FormLabel>Project role</FormLabel>
                        <Select
                          value={field.value}
                          onValueChange={field.onChange}
                          items={Object.fromEntries(
                            availableRoles.map((role) => [
                              role,
                              PROJECT_ROLE_LABELS[role],
                            ]),
                          )}
                        >
                          <FormControl>
                            <SelectTrigger className="w-full">
                              <SelectValue />
                            </SelectTrigger>
                          </FormControl>
                          <SelectContent>
                            {availableRoles.map((role) => (
                              <SelectItem key={role} value={role}>
                                {PROJECT_ROLE_LABELS[role]}
                              </SelectItem>
                            ))}
                          </SelectContent>
                        </Select>
                        {selectedUser?.role === 'qa_viewer' ? (
                          <FormDescription>
                            QA viewers always receive read-only Viewer access.
                          </FormDescription>
                        ) : null}
                        <FormMessage />
                      </FormItem>
                    )}
                  />
                  <DialogFooter className="static">
                    <Button
                      type="button"
                      variant="outline"
                      onClick={() => setDialogOpen(false)}
                    >
                      Cancel
                    </Button>
                    <Button type="submit" disabled={addMutation.isPending}>
                      {addMutation.isPending ? (
                        <>
                          <Spinner className="size-4" />
                          Adding...
                        </>
                      ) : (
                        'Add'
                      )}
                    </Button>
                  </DialogFooter>
                </form>
              </Form>
            </DialogContent>
          </Dialog>
        </CardHeader>
        <CardContent>
          {isLoading ? (
            <div className="space-y-3">
              <Skeleton className="h-16 w-full" />
              <Skeleton className="h-16 w-full" />
            </div>
          ) : error ? (
            <Alert variant="destructive">
              <AlertDescription>
                Failed to load project access: {error.message}
              </AlertDescription>
            </Alert>
          ) : members.length === 0 ? (
            <p className="py-4 text-sm text-muted-foreground">
              No explicit project members yet.
            </p>
          ) : (
            <>
              <div className="divide-y md:hidden">
                {members.map((member) => {
                  const instanceRole = member.user_role
                  return (
                    <div
                      key={member.id}
                      className="flex min-h-16 items-center gap-3 py-3"
                    >
                      <div className="min-w-0 flex-1">
                        <MemberIdentity member={member} />
                        <div className="mt-2 flex flex-wrap gap-2 pl-11">
                          <Badge variant="outline">
                            {INSTANCE_ROLE_LABELS[instanceRole] ?? 'Unknown'}
                          </Badge>
                          <Badge variant="secondary">
                            {
                              PROJECT_ROLE_LABELS[
                                instanceRole === 'qa_viewer'
                                  ? 'viewer'
                                  : member.role
                              ]
                            }
                          </Badge>
                        </div>
                      </div>
                      <MemberActions
                        member={member}
                        instanceRole={instanceRole}
                        pending={updateMutation.isPending}
                        onRoleChange={(role) => updateRole(member, role)}
                        onRemove={() => setMemberToRemove(member)}
                      />
                    </div>
                  )
                })}
              </div>
              <div className="hidden md:block">
                <Table>
                  <TableHeader>
                    <TableRow>
                      <TableHead>User</TableHead>
                      <TableHead>Instance role</TableHead>
                      <TableHead>Project role</TableHead>
                      <TableHead className="w-12">
                        <span className="sr-only">Actions</span>
                      </TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {members.map((member) => {
                      const instanceRole = member.user_role
                      return (
                        <TableRow key={member.id}>
                          <TableCell>
                            <MemberIdentity member={member} />
                          </TableCell>
                          <TableCell>
                            <Badge variant="outline">
                              {INSTANCE_ROLE_LABELS[instanceRole] ?? 'Unknown'}
                            </Badge>
                          </TableCell>
                          <TableCell>
                            {
                              PROJECT_ROLE_LABELS[
                                instanceRole === 'qa_viewer'
                                  ? 'viewer'
                                  : member.role
                              ]
                            }
                          </TableCell>
                          <TableCell>
                            <MemberActions
                              member={member}
                              instanceRole={instanceRole}
                              pending={updateMutation.isPending}
                              onRoleChange={(role) => updateRole(member, role)}
                              onRemove={() => setMemberToRemove(member)}
                            />
                          </TableCell>
                        </TableRow>
                      )
                    })}
                  </TableBody>
                </Table>
              </div>
            </>
          )}
        </CardContent>
      </Card>

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
