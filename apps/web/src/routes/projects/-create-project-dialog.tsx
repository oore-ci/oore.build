import { useMemo, useRef, useState } from 'react'
import { Link, useNavigate } from '@tanstack/react-router'
import { zodResolver } from '@hookform/resolvers/zod'
import { useForm } from 'react-hook-form'
import * as z from 'zod'
import { HugeiconsIcon } from '@hugeicons/react'
import { Folder02Icon, Link04Icon } from '@hugeicons/core-free-icons'
import { toast } from '@/lib/toast'
import { Button } from '@/components/ui/button'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Input } from '@/components/ui/input'
import LocalFolderPickerDialog from '@/components/LocalFolderPickerDialog'
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
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Spinner } from '@/components/ui/spinner'
import RepositoryAvatar from '@/components/repository-avatar'
import type { ScmProvider } from '@oore/client/models'
import { useCreateProject } from '@/hooks/use-projects'
import { useSetupStatus } from '@/hooks/use-setup'
import { useSourceRepositories } from '@/hooks/use-source-repositories'
import { isLoopbackHostname, resolveUrlHostname } from '@/lib/connectivity'
import { resolveInstanceApiBaseUrl } from '@/lib/instance-url'
import { isNearScrollEnd } from '@/lib/scroll'
import { useActiveInstance } from '@/stores/instance-store'

const createProjectSchema = z.object({
  name: z.string().min(1, 'Name is required'),
  description: z.string().optional(),
  default_branch: z.string().optional(),
  local_repository_path: z.string().optional(),
  repository_id: z.string().optional(),
})

type CreateProjectForm = z.infer<typeof createProjectSchema>

function sourceProviderLabel(provider: ScmProvider): string {
  if (provider === 'gitlab') return 'GitLab'
  if (provider === 'github') return 'GitHub'
  return 'Local Git'
}

interface CreateProjectDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
}

export default function CreateProjectDialog({
  open,
  onOpenChange,
}: CreateProjectDialogProps) {
  const titleRef = useRef<HTMLHeadingElement>(null)
  const navigate = useNavigate()
  const createMutation = useCreateProject()
  const setupStatusQuery = useSetupStatus()
  const runtimeMode = setupStatusQuery.data?.runtime_mode ?? 'local'
  const isRemoteMode = runtimeMode === 'remote'
  const instance = useActiveInstance()
  const instanceApiBaseUrl = resolveInstanceApiBaseUrl(instance)

  const uiIsLoopback = isLoopbackHostname(window.location.hostname)
  const backendIsLoopback = isLoopbackHostname(
    resolveUrlHostname(instanceApiBaseUrl),
  )
  const canBrowseLocalFs = uiIsLoopback && backendIsLoopback

  const repositoriesQuery = useSourceRepositories(open && isRemoteMode)
  const repos = repositoriesQuery.data?.pages.flatMap(
    (page) => page.repositories,
  )
  const [pickerOpen, setPickerOpen] = useState(false)

  const repoItems = useMemo(
    () =>
      Object.fromEntries(
        (repos ?? []).map((repository) => [
          repository.id,
          `${repository.full_name} · ${sourceProviderLabel(repository.provider)} (${repository.host_url})`,
        ]),
      ),
    [repos],
  )
  const hasRepos = (repos?.length ?? 0) > 0

  const form = useForm<CreateProjectForm>({
    resolver: zodResolver(createProjectSchema),
    defaultValues: {
      name: '',
      description: '',
      default_branch: '',
      local_repository_path: '',
      repository_id: '',
    },
    mode: 'onBlur',
  })

  function handleOpenPicker() {
    if (!canBrowseLocalFs) {
      toast.error('Browse is only available from localhost.')
      return
    }
    setPickerOpen(true)
  }

  function onSubmit(data: CreateProjectForm) {
    const name = data.name.trim()
    if (!name) {
      toast.error('Name is required')
      return
    }

    const source = isRemoteMode
      ? data.repository_id?.trim()
      : data.local_repository_path?.trim()
    if (!source) {
      toast.error(
        isRemoteMode
          ? 'Select a source repository before creating a project.'
          : 'Path is required.',
      )
      return
    }

    createMutation.mutate(
      {
        name,
        description: data.description?.trim() || undefined,
        ...(isRemoteMode
          ? { repository_id: source }
          : { local_repository_path: source }),
        default_branch: data.default_branch?.trim() || undefined,
      },
      {
        onSuccess: (response) => {
          toast.success('Project created')
          form.reset()
          onOpenChange(false)
          void navigate({
            to: '/projects/$projectId',
            params: { projectId: response.project.id },
          })
        },
        onError: (error) => {
          toast.error(`Failed to create project: ${error.message}`)
        },
      },
    )
  }

  function handleOpenChange(nextOpen: boolean) {
    if (!nextOpen) {
      form.reset()
      setPickerOpen(false)
    }
    onOpenChange(nextOpen)
  }

  return (
    <>
      <Dialog open={open} onOpenChange={handleOpenChange}>
        <DialogContent initialFocus={titleRef}>
          <DialogHeader>
            <DialogTitle ref={titleRef} tabIndex={-1}>
              Create project
            </DialogTitle>
            <DialogDescription>
              {isRemoteMode
                ? "Choose a repository from a connected source. Creating the project trusts its build commands to run with the runner account's macOS permissions."
                : "Choose a repository on this Mac. Creating the project trusts its build commands to run with the runner account's macOS permissions."}
            </DialogDescription>
          </DialogHeader>

          <Form {...form}>
            <form onSubmit={form.handleSubmit(onSubmit)} className="space-y-4">
              {isRemoteMode ? (
                <FormField
                  control={form.control}
                  name="repository_id"
                  render={({ field }) => (
                    <FormItem>
                      <FormLabel>Repository</FormLabel>
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
                      ) : hasRepos ? (
                        <Select
                          value={field.value}
                          onValueChange={(value) => {
                            field.onChange(value ?? '')
                            const repository = repos?.find(
                              (repo) => repo.id === value,
                            )
                            if (!repository) return
                            if (!form.getFieldState('name').isDirty) {
                              form.setValue(
                                'name',
                                repository.full_name
                                  .split('/')
                                  .filter(Boolean)
                                  .at(-1) ?? '',
                              )
                            }
                            if (!form.getFieldState('default_branch').isDirty) {
                              form.setValue(
                                'default_branch',
                                repository.default_branch ?? '',
                              )
                            }
                          }}
                          items={repoItems}
                        >
                          <FormControl>
                            <SelectTrigger>
                              <SelectValue placeholder="Select a repository..." />
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
                            {(repos ?? []).map((repo) => (
                              <SelectItem key={repo.id} value={repo.id}>
                                <RepositoryAvatar
                                  fullName={repo.full_name}
                                  avatarUrl={repo.avatar_url}
                                  repositoryId={repo.id}
                                  provider={repo.provider}
                                />
                                <span>{repo.full_name}</span>
                              </SelectItem>
                            ))}
                          </SelectContent>
                        </Select>
                      ) : (
                        <div className="space-y-3">
                          <FormDescription>
                            No repositories are available. Connect a source and
                            sync its repositories first.
                          </FormDescription>
                          <Button
                            type="button"
                            variant="outline"
                            render={<Link to="/settings/integrations" />}
                            nativeButton={false}
                          >
                            <HugeiconsIcon icon={Link04Icon} />
                            Connect source
                          </Button>
                        </div>
                      )}
                      <FormMessage />
                    </FormItem>
                  )}
                />
              ) : null}

              <FormField
                control={form.control}
                name="name"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel>Name</FormLabel>
                    <FormControl>
                      <Input placeholder="My App" {...field} />
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
                    <FormLabel>
                      Description{' '}
                      <span className="font-normal text-muted-foreground">
                        (optional)
                      </span>
                    </FormLabel>
                    <FormControl>
                      <Input
                        placeholder="A brief description of this project"
                        {...field}
                      />
                    </FormControl>
                    <FormMessage />
                  </FormItem>
                )}
              />

              {!isRemoteMode ? (
                <FormField
                  control={form.control}
                  name="local_repository_path"
                  render={({ field }) => (
                    <FormItem>
                      <FormLabel>Path</FormLabel>
                      <div className="flex flex-col gap-2 md:flex-row">
                        <FormControl>
                          <Input
                            {...field}
                            placeholder="/absolute/path/to/repository"
                            className="font-mono text-xs"
                          />
                        </FormControl>
                        {canBrowseLocalFs ? (
                          <div className="flex gap-2">
                            <Button
                              type="button"
                              variant="outline"
                              size="icon"
                              aria-label="Browse"
                              title="Browse"
                              onClick={handleOpenPicker}
                            >
                              <HugeiconsIcon icon={Folder02Icon} />
                            </Button>
                          </div>
                        ) : null}
                      </div>
                      <FormDescription>
                        Absolute path to the Git repository.
                        {!canBrowseLocalFs ? (
                          <>
                            {' '}
                            For security, folder browsing is only available from
                            localhost.
                          </>
                        ) : null}
                      </FormDescription>
                      <FormMessage />
                    </FormItem>
                  )}
                />
              ) : null}

              <FormField
                control={form.control}
                name="default_branch"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel>
                      Default branch{' '}
                      <span className="font-normal text-muted-foreground">
                        (optional)
                      </span>
                    </FormLabel>
                    <FormControl>
                      <Input placeholder="main" {...field} />
                    </FormControl>
                    <FormMessage />
                  </FormItem>
                )}
              />

              <DialogFooter>
                <Button
                  type="button"
                  variant="outline"
                  onClick={() => handleOpenChange(false)}
                >
                  Cancel
                </Button>
                <Button
                  type="submit"
                  disabled={
                    createMutation.isPending ||
                    (isRemoteMode &&
                      (repositoriesQuery.isLoading ||
                        !!repositoriesQuery.error ||
                        !hasRepos))
                  }
                >
                  {createMutation.isPending ? (
                    <>
                      <Spinner className="size-4" />
                      Creating...
                    </>
                  ) : (
                    'Create project'
                  )}
                </Button>
              </DialogFooter>
            </form>
          </Form>
        </DialogContent>
      </Dialog>

      <LocalFolderPickerDialog
        open={pickerOpen}
        onOpenChange={setPickerOpen}
        enabled={open && !isRemoteMode && canBrowseLocalFs}
        initialPath={form.getValues('local_repository_path')}
        title="Browse Local Folders"
        description="Select a Git repository folder and use it for this project."
        requireGitRepository
        selectCurrentLabel="Use Current Folder"
        selectDirectoryLabel="Use Repo"
        onSelectPath={(path) => {
          form.setValue('local_repository_path', path, {
            shouldDirty: true,
            shouldTouch: true,
            shouldValidate: true,
          })
          setPickerOpen(false)
        }}
      />
    </>
  )
}
