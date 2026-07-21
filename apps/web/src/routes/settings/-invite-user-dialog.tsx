import { zodResolver } from '@hookform/resolvers/zod'
import { useForm } from 'react-hook-form'
import { toast } from '@/lib/toast'
import * as z from 'zod'

import { useInviteUser } from '@/hooks/use-auth'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import {
  Form,
  FormControl,
  FormDescription,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
} from '@/components/ui/form'
import { Input } from '@/components/ui/input'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Spinner } from '@/components/ui/spinner'

const ROLE_OPTIONS = {
  admin: 'Admin',
  developer: 'Developer',
  qa_viewer: 'QA Viewer',
} as const

const ROLE_DESCRIPTIONS: Record<keyof typeof ROLE_OPTIONS, string> = {
  admin:
    'Can manage users, integrations, and all projects. Cannot delete the instance.',
  developer:
    'Can work on assigned projects according to their project role. Cannot create projects or manage instance settings.',
  qa_viewer:
    'Tester access to assigned project releases, install actions, and diagnostic logs.',
}

const inviteUserSchema = z.object({
  email: z.email('Enter a valid email address.'),
  role: z.enum(['admin', 'developer', 'qa_viewer']),
})

type InviteUserForm = z.infer<typeof inviteUserSchema>

export default function InviteUserDialog({
  onOpenChange,
  open,
}: {
  onOpenChange: (open: boolean) => void
  open: boolean
}) {
  const inviteMutation = useInviteUser()
  const form = useForm<InviteUserForm>({
    resolver: zodResolver(inviteUserSchema),
    defaultValues: { email: '', role: 'developer' },
  })
  const selectedRole = form.watch('role')

  function submit(values: InviteUserForm) {
    inviteMutation.mutate(values, {
      onSuccess: () => {
        const instanceUrl = window.location.origin
        void navigator.clipboard.writeText(instanceUrl).then(
          () =>
            toast.success(`${values.email} invited`, {
              description: 'Instance URL copied to clipboard.',
            }),
          () =>
            toast.success(`${values.email} invited`, {
              description: `Share this URL: ${instanceUrl}`,
            }),
        )
        form.reset()
        onOpenChange(false)
      },
      onError: (error) =>
        form.setError('root', {
          message: error instanceof Error ? error.message : 'Invite failed.',
        }),
    })
  }

  return (
    <Dialog
      open={open}
      onOpenChange={(nextOpen) => {
        onOpenChange(nextOpen)
        if (!nextOpen) form.clearErrors()
      }}
    >
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Invite a user</DialogTitle>
          <DialogDescription>
            Choose their instance role. Project access can be assigned from a
            project’s Settings tab.
          </DialogDescription>
        </DialogHeader>
        <Form {...form}>
          <form className="space-y-4" onSubmit={form.handleSubmit(submit)}>
            <FormField
              control={form.control}
              name="email"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>Email</FormLabel>
                  <FormControl>
                    <Input
                      {...field}
                      type="email"
                      autoComplete="email"
                      placeholder="name@company.com"
                    />
                  </FormControl>
                  <FormMessage />
                </FormItem>
              )}
            />
            <FormField
              control={form.control}
              name="role"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>Instance role</FormLabel>
                  <Select
                    value={field.value}
                    onValueChange={field.onChange}
                    items={ROLE_OPTIONS}
                  >
                    <FormControl>
                      <SelectTrigger className="w-full">
                        <SelectValue />
                      </SelectTrigger>
                    </FormControl>
                    <SelectContent>
                      {Object.entries(ROLE_OPTIONS).map(([value, label]) => (
                        <SelectItem key={value} value={value}>
                          {label}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                  <FormDescription>
                    {ROLE_DESCRIPTIONS[selectedRole]}
                  </FormDescription>
                  <FormMessage />
                </FormItem>
              )}
            />
            {form.formState.errors.root?.message ? (
              <Alert variant="destructive">
                <AlertDescription>
                  {form.formState.errors.root.message}
                </AlertDescription>
              </Alert>
            ) : null}
            <DialogFooter>
              <Button
                type="button"
                variant="outline"
                onClick={() => onOpenChange(false)}
              >
                Cancel
              </Button>
              <Button type="submit" disabled={inviteMutation.isPending}>
                {inviteMutation.isPending ? (
                  <>
                    <Spinner className="size-4" />
                    Inviting...
                  </>
                ) : (
                  'Send invite'
                )}
              </Button>
            </DialogFooter>
          </form>
        </Form>
      </DialogContent>
    </Dialog>
  )
}
