import { useForm } from 'react-hook-form'
import { Link } from '@tanstack/react-router'
import { zodResolver } from '@hookform/resolvers/zod'
import { toast } from '@/lib/toast'
import * as z from 'zod'

import { useUpdateProject } from '@/hooks/use-projects'
import { useSourceRepositories } from '@/hooks/use-source-repositories'
import RepositoryAvatar from '@/components/repository-avatar'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { Card, CardContent } from '@/components/ui/card'
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
import { Spinner } from '@/components/ui/spinner'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'

const editProjectSchema = z.object({
  name: z.string().min(1, 'Name is required'),
  description: z.string().optional(),
  default_branch: z.string().optional(),
  repository_id: z.string().min(1, 'Choose a source repository'),
})

type EditProjectForm = z.infer<typeof editProjectSchema>

export function ProjectSettingsForm({
  projectId,
  currentValues,
}: {
  projectId: string
  currentValues: {
    name: string
    description?: string
    default_branch?: string
    repository_id?: string
    repository_full_name?: string
  }
}) {
  const updateMutation = useUpdateProject()
  const sourceRepositoriesQuery = useSourceRepositories(true)
  const repositories = sourceRepositoriesQuery.data ?? []
  const currentRepositoryMissing =
    !!currentValues.repository_id &&
    !repositories.some(
      (repository) => repository.id === currentValues.repository_id,
    )
  const repositoryItems = Object.fromEntries([
    ...repositories.map((repository) => [repository.id, repository.full_name]),
    ...(currentRepositoryMissing
      ? [
          [
            currentValues.repository_id!,
            currentValues.repository_full_name ?? 'Current repository',
          ],
        ]
      : []),
  ])
  const form = useForm<EditProjectForm>({
    resolver: zodResolver(editProjectSchema),
    defaultValues: {
      name: currentValues.name,
      description: currentValues.description ?? '',
      default_branch: currentValues.default_branch ?? '',
      repository_id: currentValues.repository_id ?? '',
    },
    values: {
      name: currentValues.name,
      description: currentValues.description ?? '',
      default_branch: currentValues.default_branch ?? '',
      repository_id: currentValues.repository_id ?? '',
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
          repository_id: data.repository_id,
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
              name="repository_id"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>Source repository</FormLabel>
                  <Select
                    value={field.value}
                    onValueChange={field.onChange}
                    disabled={
                      sourceRepositoriesQuery.isLoading ||
                      sourceRepositoriesQuery.isError ||
                      (sourceRepositoriesQuery.isSuccess &&
                        repositories.length === 0) ||
                      updateMutation.isPending
                    }
                    items={repositoryItems}
                  >
                    <FormControl>
                      <SelectTrigger className="w-full">
                        <SelectValue
                          placeholder={
                            sourceRepositoriesQuery.isLoading
                              ? 'Loading repositories...'
                              : 'Choose a repository'
                          }
                        />
                      </SelectTrigger>
                    </FormControl>
                    <SelectContent>
                      {currentRepositoryMissing ? (
                        <SelectItem value={currentValues.repository_id!}>
                          {currentValues.repository_full_name ??
                            'Current repository'}{' '}
                          (currently linked)
                        </SelectItem>
                      ) : null}
                      {repositories.map((repository) => (
                        <SelectItem key={repository.id} value={repository.id}>
                          <RepositoryAvatar
                            fullName={repository.full_name}
                            repositoryId={repository.id}
                            provider={repository.provider}
                            size="sm"
                          />
                          {repository.full_name}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                  <FormDescription>
                    Relinking changes the repository used for future workflow
                    discovery and builds.
                  </FormDescription>
                  <FormMessage />
                </FormItem>
              )}
            />
            {sourceRepositoriesQuery.error ? (
              <Alert variant="destructive">
                <AlertDescription className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
                  <span>Source repositories could not be loaded.</span>
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    onClick={() => void sourceRepositoriesQuery.refetch()}
                  >
                    Retry
                  </Button>
                </AlertDescription>
              </Alert>
            ) : null}
            {sourceRepositoriesQuery.isSuccess &&
            repositories.length === 0 &&
            !currentValues.repository_id ? (
              <Alert>
                <AlertDescription className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
                  <span>No source repositories are available to link.</span>
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    render={<Link to="/settings/integrations" />}
                  >
                    Open sources
                  </Button>
                </AlertDescription>
              </Alert>
            ) : null}
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
