import { zodResolver } from '@hookform/resolvers/zod'
import { useForm } from 'react-hook-form'
import { toast } from 'sonner'
import z from 'zod'

import { useUpdateProject } from '@/hooks/use-projects'
import { Button } from '@/components/ui/button'
import { Card, CardContent } from '@/components/ui/card'
import {
  Form,
  FormControl,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
} from '@/components/ui/form'
import { Input } from '@/components/ui/input'
import { Spinner } from '@/components/ui/spinner'

const editProjectSchema = z.object({
  name: z.string().min(1, 'Name is required'),
  description: z.string().optional(),
  default_branch: z.string().optional(),
})

type EditProjectForm = z.infer<typeof editProjectSchema>

export function ProjectSettingsForm({
  projectId,
  currentValues,
}: {
  projectId: string
  currentValues: { name: string; description?: string; default_branch?: string }
}) {
  const updateMutation = useUpdateProject()
  const form = useForm<EditProjectForm>({
    resolver: zodResolver(editProjectSchema),
    defaultValues: {
      name: currentValues.name,
      description: currentValues.description ?? '',
      default_branch: currentValues.default_branch ?? '',
    },
    values: {
      name: currentValues.name,
      description: currentValues.description ?? '',
      default_branch: currentValues.default_branch ?? '',
    },
    mode: 'onBlur',
  })

  function onSubmit(data: EditProjectForm) {
    updateMutation.mutate(
      {
        projectId,
        data: {
          name: data.name.trim(),
          description: data.description?.trim() || undefined,
          default_branch: data.default_branch?.trim() || undefined,
        },
      },
      {
        onSuccess: () => toast.success('Project updated'),
        onError: (error) =>
          toast.error(`Failed to update project: ${error.message}`),
      },
    )
  }

  return (
    <Card>
      <CardContent>
        <Form {...form}>
          <form onSubmit={form.handleSubmit(onSubmit)} className="space-y-4">
            <FormField
              control={form.control}
              name="name"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>Name</FormLabel>
                  <FormControl>
                    <Input {...field} />
                  </FormControl>
                  <FormMessage />
                </FormItem>
              )}
            />
            <FormField
              control={form.control}
              name="description"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>Description</FormLabel>
                  <FormControl>
                    <Input {...field} />
                  </FormControl>
                  <FormMessage />
                </FormItem>
              )}
            />
            <FormField
              control={form.control}
              name="default_branch"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>Default Branch</FormLabel>
                  <FormControl>
                    <Input {...field} />
                  </FormControl>
                  <FormMessage />
                </FormItem>
              )}
            />
            <div className="flex justify-end">
              <Button type="submit" disabled={updateMutation.isPending}>
                {updateMutation.isPending ? (
                  <>
                    <Spinner className="size-4" />
                    Saving...
                  </>
                ) : (
                  'Save'
                )}
              </Button>
            </div>
          </form>
        </Form>
      </CardContent>
    </Card>
  )
}
