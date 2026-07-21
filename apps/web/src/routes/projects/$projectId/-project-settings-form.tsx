import { Link } from '@tanstack/react-router'
import { useForm } from 'react-hook-form'
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
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Spinner } from '@/components/ui/spinner'
import { SourceDiscoveryWarning } from '@/components/source-discovery-warning'

const editProjectSchema = z.object({
  name: z.string().min(1, 'Name is required'),
  description: z.string().optional(),
  default_branch: z.string().optional(),
  repository_id: z.string().optional(),
})

type EditProjectForm = z.infer<typeof editProjectSchema>

export function ProjectSettingsForm({
  canChangeSource,
  projectId,
  currentValues,
}: {
  canChangeSource: boolean
  projectId: string
  currentValues: {
    name: string
    description?: string
    default_branch?: string
    repository_id?: string
  }
}) {
  const updateMutation = useUpdateProject()
  const repositoriesQuery = useSourceRepositories(canChangeSource)
  const repositories = repositoriesQuery.data ?? []
  const repositoryItems = Object.fromEntries(
    repositories.map((repository) => [repository.id, repository.full_name]),
  )
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
          repository_id:
            canChangeSource && form.getFieldState('repository_id').isDirty
              ? data.repository_id?.trim() || undefined
              : undefined,
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
            {canChangeSource ? (
              <FormField
                control={form.control}
                name="repository_id"
                render={({ field, fieldState }) => (
                  <FormItem>
                    <FormLabel>Source repository</FormLabel>
                    <SourceDiscoveryWarning
                      failures={repositoriesQuery.sourceFailures}
                      isRetrying={repositoriesQuery.isFetching}
                      onRetry={() => void repositoriesQuery.refetch()}
                    />
                    {repositoriesQuery.isLoading ? (
                      <div className="flex items-center gap-2 py-2">
                        <Spinner className="size-4" />
                        <span className="text-sm text-muted-foreground">
                          Loading repositories...
                        </span>
                      </div>
                    ) : repositoriesQuery.error ? (
                      <Alert variant="destructive">
                        <AlertDescription className="flex items-center justify-between gap-3">
                          <span>
                            Failed to load repositories:{' '}
                            {repositoriesQuery.error.message}
                          </span>
                          <Button
                            type="button"
                            size="sm"
                            variant="outline"
                            onClick={() => void repositoriesQuery.refetch()}
                          >
                            Retry
                          </Button>
                        </AlertDescription>
                      </Alert>
                    ) : repositories.length > 0 ? (
                      <Select
                        value={field.value}
                        onValueChange={(value) => field.onChange(value ?? '')}
                        items={repositoryItems}
                      >
                        <FormControl>
                          <SelectTrigger>
                            <SelectValue placeholder="Choose source" />
                          </SelectTrigger>
                        </FormControl>
                        <SelectContent>
                          {repositories.map((repository) => (
                            <SelectItem
                              key={repository.id}
                              value={repository.id}
                            >
                              <RepositoryAvatar
                                fullName={repository.full_name}
                                avatarUrl={repository.avatar_url}
                                repositoryId={repository.id}
                                provider={repository.provider}
                              />
                              <span>{repository.full_name}</span>
                            </SelectItem>
                          ))}
                        </SelectContent>
                      </Select>
                    ) : (
                      <Button
                        type="button"
                        variant="outline"
                        render={<Link to="/settings/integrations" />}
                        nativeButton={false}
                      >
                        Connect source
                      </Button>
                    )}
                    <FormDescription>
                      Choosing the source trusts its build commands to run with
                      the runner account&apos;s macOS permissions.
                    </FormDescription>
                    {fieldState.isDirty &&
                    currentValues.repository_id &&
                    field.value !== currentValues.repository_id ? (
                      <Alert>
                        <AlertDescription>
                          Changing the source from{' '}
                          {repositoryItems[currentValues.repository_id] ??
                            'the current repository'}{' '}
                          to{' '}
                          {repositoryItems[field.value ?? ''] ??
                            'the selected repository'}{' '}
                          cancels queued or scheduled builds from the old
                          source. Assigned or running builds finish from their
                          original source.
                        </AlertDescription>
                      </Alert>
                    ) : null}
                    <FormMessage />
                  </FormItem>
                )}
              />
            ) : null}
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
