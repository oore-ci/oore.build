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
  Drawer,
  DrawerContent,
  DrawerDescription,
  DrawerFooter,
  DrawerHeader,
  DrawerTitle,
} from '@/components/ui/drawer'
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
import { useFirstAppScope, useFirstAppStore } from '@/stores/first-app-store'

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
  const scope = useFirstAppScope()
  const draft = useFirstAppStore((state) => state.progress[scope]?.projectDraft)
  const updateProgress = useFirstAppStore((state) => state.update)
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
      name: draft?.name ?? '',
      description: draft?.description ?? '',
      default_branch: draft?.default_branch ?? '',
      local_repository_path: draft?.local_repository_path ?? '',
      repository_id: draft?.repository_id ?? '',
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
          updateProgress(scope, {
            projectId: response.project.id,
            hidden: false,
            projectDraft: undefined,
          })
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
      updateProgress(scope, { projectDraft: undefined })
      form.reset()
      setPickerOpen(false)
    }
    onOpenChange(nextOpen)
  }

  return (
    <>
      <Drawer
        swipeDirection="right"
        open={open}
        onOpenChange={handleOpenChange}
      >
        <DrawerContent
          initialFocus={titleRef}
          className="data-[swipe-axis=x]:[--drawer-content-width:calc(100%-1rem)] data-[swipe-axis=x]:sm:[--drawer-content-width:30rem]"
        >
          <DrawerHeader>
            <DrawerTitle ref={titleRef} tabIndex={-1} className="outline-none">
              Create project
            </DrawerTitle>
            <DrawerDescription>
              {isRemoteMode
                ? "Choose a repository from a connected source. Creating the project trusts its build commands to run with the runner account's macOS permissions."
                : "Choose a repository on this Mac. Creating the project trusts its build commands to run with the runner account's macOS permissions."}
            </DrawerDescription>
          </DrawerHeader>

          <Form {...form}>
            <form
              onSubmit={form.handleSubmit(onSubmit)}
              className="flex min-h-0 flex-1 flex-col"
            >
              <div className="flex-1 scroll-fade space-y-4 overflow-y-auto p-4">
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
                              if (
                                !form.getFieldState('name').isDirty &&
                                !draft?.name
                              ) {
                                form.setValue(
                                  'name',
                                  repository.full_name
                                    .split('/')
                                    .filter(Boolean)
                                    .at(-1) ?? '',
                                )
                              }
                              if (
                                !form.getFieldState('default_branch').isDirty &&
                                !draft?.default_branch
                              ) {
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
                              No repositories are available. Connect a source
                              and sync its repositories first.
                            </FormDescription>
                            <Button
                              type="button"
                              variant="outline"
                              render={<Link to="/settings/integrations" />}
                              nativeButton={false}
                              onClick={() =>
                                updateProgress(scope, {
                                  projectDraft: form.getValues(),
                                })
                              }
                            >
                              <HugeiconsIcon icon={Link04Icon} />
                              Connect source
                            </Button>
                          </div>
                        )}
                        {hasRepos ? (
                          <Button
                            type="button"
                            variant="link"
                            size="sm"
                            className="h-auto p-0"
                            render={<Link to="/settings/integrations" />}
                            nativeButton={false}
                            onClick={() =>
                              updateProgress(scope, {
                                projectDraft: form.getValues(),
                              })
                            }
                          >
                            Connect another source
                          </Button>
                        ) : null}
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
                              For security, folder browsing is only available
                              from localhost.
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
              </div>
              <DrawerFooter>
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
              </DrawerFooter>
            </form>
          </Form>
        </DrawerContent>
      </Drawer>

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
