import { zodResolver } from '@hookform/resolvers/zod'
import { Delete02Icon } from '@hugeicons/core-free-icons'
import { HugeiconsIcon } from '@hugeicons/react'
import { useMemo, useState } from 'react'
import { useForm } from 'react-hook-form'
import { toast } from 'sonner'
import * as z from 'zod'

import type { ProjectMember, ProjectRole, User } from '@/lib/types'
import { useUsers } from '@/hooks/use-auth'
import {
  useAddProjectMember,
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
  user_id: z.string().min(1, 'Select a user'),
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

function initials(user: Pick<User, 'email' | 'display_name'>): string {
  const source = user.display_name?.trim() || user.email
  return source
    .split(/[\s@._-]+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((part) => part[0]?.toUpperCase())
    .join('')
}

export function ProjectAccessCard({ projectId }: { projectId: string }) {
  const membersQuery = useProjectMembers(projectId)
  const usersQuery = useUsers()
  const addMutation = useAddProjectMember(projectId)
  const updateMutation = useUpdateProjectMember(projectId)
  const removeMutation = useRemoveProjectMember(projectId)
  const [memberToRemove, setMemberToRemove] = useState<ProjectMember | null>(
    null,
  )
  const form = useForm<AccessForm>({
    resolver: zodResolver(accessSchema),
    defaultValues: { user_id: '', role: 'viewer' },
  })

  const users = useMemo(() => usersQuery.data?.users ?? [], [usersQuery.data])
  const members = useMemo(
    () => membersQuery.data?.members ?? [],
    [membersQuery.data],
  )
  const usersById = useMemo(
    () => new Map(users.map((user) => [user.id, user])),
    [users],
  )
  const memberIds = useMemo(
    () => new Set(members.map((member) => member.user_id)),
    [members],
  )
  const availableUsers = useMemo(
    () =>
      users.filter(
        (user) =>
          (user.status === 'active' || user.status === 'invited') &&
          (user.role === 'developer' || user.role === 'qa_viewer') &&
          !memberIds.has(user.id),
      ),
    [memberIds, users],
  )
  const selectedUser = usersById.get(form.watch('user_id'))
  const availableRoles =
    selectedUser?.role === 'qa_viewer'
      ? (['viewer'] as Array<ProjectRole>)
      : PROJECT_ROLE_OPTIONS

  function onSubmit(values: AccessForm) {
    const user = usersById.get(values.user_id)
    const role: ProjectRole =
      user?.role === 'qa_viewer' ? 'viewer' : values.role
    addMutation.mutate(
      { user_id: values.user_id, role },
      {
        onSuccess: () => {
          toast.success(`${user?.email ?? 'User'} added to this project`)
          form.reset({ user_id: '', role: 'viewer' })
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

  const isLoading = membersQuery.isLoading || usersQuery.isLoading
  const error = membersQuery.error ?? usersQuery.error

  return (
    <>
      <Card>
        <CardHeader>
          <CardTitle>Project access</CardTitle>
          <CardDescription>
            Assign developers and QA viewers to this project. QA access is
            always read-only and includes build logs and installable artifacts.
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-5">
          {isLoading ? (
            <div className="space-y-3">
              <Skeleton className="h-9 w-full" />
              <Skeleton className="h-24 w-full" />
            </div>
          ) : error ? (
            <Alert variant="destructive">
              <AlertDescription>
                Failed to load project access: {error.message}
              </AlertDescription>
            </Alert>
          ) : (
            <>
              <Form {...form}>
                <form
                  onSubmit={form.handleSubmit(onSubmit)}
                  className="grid items-start gap-3 md:grid-cols-[minmax(0,1fr)_12rem_auto]"
                >
                  <FormField
                    control={form.control}
                    name="user_id"
                    render={({ field }) => (
                      <FormItem>
                        <FormLabel>User</FormLabel>
                        <Select
                          value={field.value}
                          onValueChange={(value) => {
                            field.onChange(value)
                            const user = usersById.get(value ?? '')
                            if (user?.role === 'qa_viewer') {
                              form.setValue('role', 'viewer')
                            }
                          }}
                          items={Object.fromEntries(
                            availableUsers.map((user) => [
                              user.id,
                              `${user.email} · ${INSTANCE_ROLE_LABELS[user.role]}`,
                            ]),
                          )}
                          disabled={availableUsers.length === 0}
                        >
                          <FormControl>
                            <SelectTrigger className="w-full">
                              <SelectValue
                                placeholder={
                                  availableUsers.length === 0
                                    ? 'Everyone eligible already has access'
                                    : 'Select a user'
                                }
                              />
                            </SelectTrigger>
                          </FormControl>
                          <SelectContent>
                            {availableUsers.map((user) => (
                              <SelectItem key={user.id} value={user.id}>
                                {user.email}
                                <span className="text-xs text-muted-foreground">
                                  {INSTANCE_ROLE_LABELS[user.role]}
                                  {user.status === 'invited'
                                    ? ' · Invited'
                                    : ''}
                                </span>
                              </SelectItem>
                            ))}
                          </SelectContent>
                        </Select>
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
                          onValueChange={(value) => field.onChange(value)}
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
                            QA Viewers are always assigned read-only access.
                          </FormDescription>
                        ) : null}
                        <FormMessage />
                      </FormItem>
                    )}
                  />
                  <Button
                    type="submit"
                    className="md:mt-7"
                    disabled={
                      availableUsers.length === 0 || addMutation.isPending
                    }
                  >
                    {addMutation.isPending ? (
                      <>
                        <Spinner className="size-4" />
                        Adding...
                      </>
                    ) : (
                      'Add access'
                    )}
                  </Button>
                </form>
              </Form>

              <div className="overflow-hidden rounded-md border">
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
                    {members.length === 0 ? (
                      <TableRow>
                        <TableCell
                          colSpan={4}
                          className="text-center text-muted-foreground"
                        >
                          No explicit project members yet.
                        </TableCell>
                      </TableRow>
                    ) : (
                      members.map((member) => {
                        const user = usersById.get(member.user_id)
                        const instanceRole = user?.role
                        const hasImplicitAccess =
                          instanceRole === 'owner' || instanceRole === 'admin'
                        const isQaViewer = instanceRole === 'qa_viewer'
                        return (
                          <TableRow key={member.id}>
                            <TableCell>
                              <div className="flex items-center gap-3">
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
                                    {member.user_display_name ??
                                      member.user_email}
                                  </p>
                                  {member.user_display_name ? (
                                    <p className="truncate text-xs text-muted-foreground">
                                      {member.user_email}
                                    </p>
                                  ) : null}
                                </div>
                              </div>
                            </TableCell>
                            <TableCell>
                              <Badge variant="outline">
                                {INSTANCE_ROLE_LABELS[instanceRole ?? ''] ??
                                  'Unknown'}
                              </Badge>
                            </TableCell>
                            <TableCell>
                              <Select
                                value={isQaViewer ? 'viewer' : member.role}
                                onValueChange={(value) =>
                                  updateRole(member, value as ProjectRole)
                                }
                                items={Object.fromEntries(
                                  (isQaViewer
                                    ? (['viewer'] as Array<ProjectRole>)
                                    : PROJECT_ROLE_OPTIONS
                                  ).map((role) => [
                                    role,
                                    PROJECT_ROLE_LABELS[role],
                                  ]),
                                )}
                                disabled={
                                  hasImplicitAccess ||
                                  isQaViewer ||
                                  updateMutation.isPending
                                }
                              >
                                <SelectTrigger size="sm" className="w-32">
                                  <SelectValue />
                                </SelectTrigger>
                                <SelectContent>
                                  {(isQaViewer
                                    ? (['viewer'] as Array<ProjectRole>)
                                    : PROJECT_ROLE_OPTIONS
                                  ).map((role) => (
                                    <SelectItem key={role} value={role}>
                                      {PROJECT_ROLE_LABELS[role]}
                                    </SelectItem>
                                  ))}
                                </SelectContent>
                              </Select>
                            </TableCell>
                            <TableCell>
                              {!hasImplicitAccess ? (
                                <Button
                                  variant="ghost"
                                  size="icon-sm"
                                  onClick={() => setMemberToRemove(member)}
                                  aria-label={`Remove ${member.user_email} from project`}
                                  title="Remove project access"
                                >
                                  <HugeiconsIcon icon={Delete02Icon} />
                                </Button>
                              ) : null}
                            </TableCell>
                          </TableRow>
                        )
                      })
                    )}
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
              {memberToRemove?.user_email} will no longer see this project's
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
