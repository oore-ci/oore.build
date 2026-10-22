import { Link } from '@tanstack/react-router'
import { useForm } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import { toast } from '@/lib/toast'
import * as z from 'zod'

import { useUpdateProject } from '@/hooks/use-projects'
import { useSourceRepositories } from '@/hooks/use-source-repositories'
import { isNearScrollEnd } from '@/lib/scroll'
import RepositoryAvatar from '@/components/repository-avatar'
import { FormActions } from '@/components/form-actions'
import {
  SettingsSection,
  SettingsSurface,
  SettingsSurfaceSeparator,
} from '@/components/settings/settings-section'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
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
  currentValues: EditProjectForm & { repository_full_name?: string }
}) {
  const updateMutation = useUpdateProject()
  const repositoriesQuery = useSourceRepositories(canChangeSource)
  const repositories =
    repositoriesQuery.data?.pages.flatMap((page) => page.repositories) ?? []
  const repositoryItems = Object.fromEntries(
    repositories.map((repository) => [repository.id, repository.full_name]),
  )
  const form = useForm<EditProjectForm>({
    resolver: zodResolver(editProjectSchema),
    values: {
      name: currentValues.name,
      description: currentValues.description ?? '',
      default_branch: currentValues.default_branch ?? '',
      repository_id: currentValues.repository_id ?? '',
    },
    mode: 'onBlur',
  })

  const { isDirty } = form.formState

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
    <Form {...form}>
      <form
        onSubmit={form.handleSubmit(onSubmit)}
        className="flex flex-col gap-8"
      >
        <SettingsSection
          title="General"
          description="How this project appears across Oore and the repository it builds from."
        >
          <SettingsSurface className="flex flex-col gap-4">
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
            <SettingsSurfaceSeparator />
            <FormField
              control={form.control}
              name="default_branch"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>Default branch</FormLabel>
                  <FormControl>
                    <Input {...field} className="font-mono" />
                  </FormControl>
                  <FormDescription>
                    Preselected when you run a build and used to discover
                    checked-in workflows.
                  </FormDescription>
                  <FormMessage />
                </FormItem>
              )}
            />
            <SettingsSurfaceSeparator />
            {canChangeSource ? (
              <FormField
                control={form.control}
                name="repository_id"
                render={({ field, fieldState }) => (
                  <FormItem>
                    <FormLabel>Source repository</FormLabel>
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
                          <SelectTrigger className="w-full">
                            <SelectValue placeholder="Choose a repository" />
                          </SelectTrigger>
                        </FormControl>
                        <SelectContent
                          onScroll={(event) => {
                            const target = event.currentTarget
                            if (
                              isNearScrollEnd(target) &&
                              repositoriesQuery.hasNextPage &&
                              !repositoriesQuery.isFetchingNextPage
                            ) {
                              void repositoriesQuery.fetchNextPage()
                            }
                          }}
                        >
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
                      <div className="flex flex-col items-start gap-2">
                        <p className="text-sm text-muted-foreground">
                          No source is connected yet. Connect GitHub, GitLab or
                          a local folder first.
                        </p>
                        <Button
                          type="button"
                          variant="outline"
                          render={<Link to="/settings/integrations" />}
                          nativeButton={false}
                        >
                          Connect source
                        </Button>
                      </div>
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
            ) : (
              <div className="flex flex-col gap-1.5 text-sm">
                <p className="font-medium">Source repository</p>
                <p>
                  {currentValues.repository_full_name ? (
                    <span className="font-medium">
                      {currentValues.repository_full_name}
                    </span>
                  ) : (
                    'No repository connected.'
                  )}{' '}
                  <span className="text-muted-foreground">
                    Only an owner or admin can change the source.
                  </span>
                </p>
              </div>
            )}
            <SettingsSurfaceSeparator />
            <FormActions
              isPending={updateMutation.isPending}
              disabled={!isDirty}
              disabledReason="No changes to save"
              onCancel={isDirty ? () => form.reset() : undefined}
              cancelLabel="Discard"
              submitLabel="Save changes"
            />
          </SettingsSurface>
        </SettingsSection>
      </form>
    </Form>
  )
}
