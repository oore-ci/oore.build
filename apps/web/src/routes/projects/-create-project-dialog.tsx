import { useEffect, useMemo, useState } from 'react'
import { useForm } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import { z } from 'zod'
import { useNavigate } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  Add01Icon,
  ArrowUp01Icon,
  Folder02Icon,
  GitBranchIcon,
  Refresh01Icon,
} from '@hugeicons/core-free-icons'
import { toast } from 'sonner'

import { useQuery } from '@tanstack/react-query'
import type { IntegrationRepository } from '@/lib/types'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
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
import { Badge } from '@/components/ui/badge'
import { ScrollArea } from '@/components/ui/scroll-area'
import { Spinner } from '@/components/ui/spinner'
import { useCreateProject } from '@/hooks/use-projects'
import { useBrowseLocalGitDirectories } from '@/hooks/use-integrations'
import { useSetupStatus } from '@/hooks/use-setup'
import { listIntegrationRepos, listIntegrations } from '@/lib/api'
import { useActiveInstance } from '@/stores/instance-store'
import { useAuthStore } from '@/stores/auth-store'

const createProjectSchema = z.object({
  name: z.string().min(1, 'Name is required'),
  description: z.string().optional(),
  default_branch: z.string().optional(),
  local_repository_path: z.string().optional(),
})

type CreateProjectForm = z.infer<typeof createProjectSchema>

interface CreateProjectDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
}

function useAvailableRepos(enabled: boolean) {
  const instance = useActiveInstance()
  const token = useAuthStore((s) => s.token)
  const baseUrl = instance?.url ?? null

  return useQuery({
    queryKey: [instance?.id ?? '__none__', 'all-repos-for-project'],
    queryFn: async () => {
      if (!baseUrl || !token) return []
      const intResp = await listIntegrations(baseUrl, token)
      const repos: Array<IntegrationRepository> = []
      for (const integration of intResp.integrations) {
        try {
          const repoResp = await listIntegrationRepos(
            baseUrl,
            token,
            integration.id,
          )
          repos.push(...repoResp.repositories)
        } catch {
          // skip failed sources
        }
      }
      return repos
    },
    enabled: enabled && !!baseUrl && !!token,
  })
}

export default function CreateProjectDialog({
  open,
  onOpenChange,
}: CreateProjectDialogProps) {
  const navigate = useNavigate()
  const createMutation = useCreateProject()
  const setupStatusQuery = useSetupStatus()
  const runtimeMode = setupStatusQuery.data?.runtime_mode ?? 'remote'
  const isLocalMode = runtimeMode === 'local'

  const { data: repos, isLoading: reposLoading } = useAvailableRepos(
    open && !isLocalMode,
  )
  const [selectedRepoId, setSelectedRepoId] = useState<string>('')
  const [pickerOpen, setPickerOpen] = useState(false)
  const [browserPath, setBrowserPath] = useState<string | undefined>(undefined)
  const repoItems = useMemo(
    () => Object.fromEntries((repos ?? []).map((r) => [r.id, r.full_name])),
    [repos],
  )
  const hasRepos = (repos?.length ?? 0) > 0

  const {
    data: browserData,
    isLoading: browserLoading,
    isFetching: browserFetching,
    refetch: refetchBrowser,
  } = useBrowseLocalGitDirectories(browserPath, open && isLocalMode)

  const form = useForm<CreateProjectForm>({
    resolver: zodResolver(createProjectSchema),
    defaultValues: {
      name: '',
      description: '',
      default_branch: '',
      local_repository_path: '',
    },
    mode: 'onBlur',
  })

  useEffect(() => {
    if (!open) return
    if (isLocalMode) return
    if (!hasRepos) return
    if (selectedRepoId) return
    setSelectedRepoId(repos![0].id)
  }, [open, isLocalMode, hasRepos, repos, selectedRepoId])

  function applyLocalPath(path: string, closePicker = false) {
    form.setValue('local_repository_path', path, {
      shouldDirty: true,
      shouldTouch: true,
      shouldValidate: true,
    })
    if (closePicker) {
      setPickerOpen(false)
    }
  }

  function handleOpenPicker() {
    const candidate = form.getValues('local_repository_path')?.trim()
    setBrowserPath(candidate ? candidate : undefined)
    setPickerOpen(true)
  }

  function onSubmit(data: CreateProjectForm) {
    const name = data.name.trim()
    if (!name) {
      toast.error('Name is required')
      return
    }

    if (isLocalMode) {
      const localRepositoryPath = data.local_repository_path?.trim()
      if (!localRepositoryPath) {
        toast.error('Path is required.')
        return
      }

      createMutation.mutate(
        {
          name,
          description: data.description?.trim() || undefined,
          local_repository_path: localRepositoryPath,
          default_branch: data.default_branch?.trim() || undefined,
        },
        {
          onSuccess: (res) => {
            toast.success('Project created')
            form.reset()
            setBrowserPath(undefined)
            onOpenChange(false)
            void navigate({
              to: '/projects/$projectId',
              params: { projectId: res.project.id },
            })
          },
          onError: (err) => {
            toast.error(`Failed to create project: ${err.message}`)
          },
        },
      )
      return
    }

    if (!selectedRepoId) {
      toast.error('Select a source repository before creating a project.')
      return
    }

    createMutation.mutate(
      {
        name,
        description: data.description?.trim() || undefined,
        repository_id: selectedRepoId,
        default_branch: data.default_branch?.trim() || undefined,
      },
      {
        onSuccess: (res) => {
          toast.success('Project created')
          form.reset()
          setSelectedRepoId('')
          onOpenChange(false)
          void navigate({
            to: '/projects/$projectId',
            params: { projectId: res.project.id },
          })
        },
        onError: (err) => {
          toast.error(`Failed to create project: ${err.message}`)
        },
      },
    )
  }

  function handleOpenChange(nextOpen: boolean) {
    if (!nextOpen) {
      form.reset()
      setSelectedRepoId('')
      setPickerOpen(false)
      setBrowserPath(undefined)
    }
    onOpenChange(nextOpen)
  }

  return (
    <>
      <Dialog open={open} onOpenChange={handleOpenChange}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Create Project</DialogTitle>
            <DialogDescription>
              {isLocalMode
                ? 'Create a project from a path.'
                : 'Create a project linked to a connected source repository.'}
            </DialogDescription>
          </DialogHeader>

          <Form {...form}>
            <form onSubmit={form.handleSubmit(onSubmit)} className="space-y-4">
              <FormField
                control={form.control}
                name="name"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel>Name</FormLabel>
                    <FormControl>
                      <Input placeholder="My App" autoFocus {...field} />
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
                      <span className="text-muted-foreground font-normal">
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

              {isLocalMode ? (
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
                        <div className="flex gap-2">
                          <Button
                            type="button"
                            variant="outline"
                            size="icon"
                            aria-label="Browse"
                            title="Browse"
                            onClick={handleOpenPicker}
                          >
                            <HugeiconsIcon icon={Folder02Icon} size={16} />
                          </Button>
                        </div>
                      </div>
                      <FormDescription>
                        Absolute path to the Git repository.
                      </FormDescription>
                      <FormMessage />
                    </FormItem>
                  )}
                />
              ) : (
                <div className="space-y-2">
                  <FormLabel>Source repository</FormLabel>
                  {reposLoading ? (
                    <div className="flex items-center gap-2 py-2">
                      <Spinner className="size-4" />
                      <span className="text-sm text-muted-foreground">
                        Loading source repositories...
                      </span>
                    </div>
                  ) : hasRepos ? (
                    <Select
                      value={selectedRepoId}
                      onValueChange={(v) => setSelectedRepoId(v ?? '')}
                      items={repoItems}
                    >
                      <SelectTrigger>
                        <SelectValue placeholder="Select a repository..." />
                      </SelectTrigger>
                      <SelectContent>
                        {(repos ?? []).map((repo) => (
                          <SelectItem key={repo.id} value={repo.id}>
                            {repo.full_name}
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                  ) : (
                    <p className="text-sm text-muted-foreground">
                      No source repositories available. Connect a source and sync
                      repositories first.
                    </p>
                  )}
                </div>
              )}

              <FormField
                control={form.control}
                name="default_branch"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel>
                      Default Branch{' '}
                      <span className="text-muted-foreground font-normal">
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
                    (!isLocalMode && (reposLoading || !hasRepos))
                  }
                >
                  {createMutation.isPending ? (
                    <>
                      <Spinner className="size-4" />
                      Creating...
                    </>
                  ) : (
                    'Create'
                  )}
                </Button>
              </DialogFooter>
            </form>
          </Form>
        </DialogContent>
      </Dialog>

      <Dialog open={pickerOpen} onOpenChange={setPickerOpen}>
        <DialogContent className="sm:max-w-4xl">
          <DialogHeader>
            <DialogTitle>Browse Local Folders</DialogTitle>
            <DialogDescription>
              Select a Git repository folder and use it for this project.
            </DialogDescription>
          </DialogHeader>

          <div className="space-y-4">
            <div className="rounded-md border bg-muted/20 p-3">
              <p className="text-xs text-muted-foreground">Current folder</p>
              <p className="mt-1 break-all font-mono text-xs">
                {browserData?.current_path ?? 'Loading...'}
              </p>
            </div>

            <div className="flex flex-wrap items-center gap-2">
              <Button
                type="button"
                variant="outline"
                size="sm"
                onClick={() => {
                  if (browserData?.parent_path) {
                    setBrowserPath(browserData.parent_path)
                  }
                }}
                disabled={!browserData?.parent_path || browserFetching}
              >
                <HugeiconsIcon icon={ArrowUp01Icon} size={14} />
                Up
              </Button>

              <Button
                type="button"
                variant="outline"
                size="sm"
                onClick={() => void refetchBrowser()}
                disabled={browserFetching}
              >
                <HugeiconsIcon icon={Refresh01Icon} size={14} />
                Refresh
              </Button>

              {browserData?.current_is_git_repository ? (
                <Button
                  type="button"
                  size="sm"
                  onClick={() => applyLocalPath(browserData.current_path, true)}
                >
                  <HugeiconsIcon icon={GitBranchIcon} size={14} />
                  Use Current Folder
                </Button>
              ) : null}
            </div>

            {browserLoading ? (
              <div className="flex items-center gap-2 py-3">
                <Spinner className="size-4" />
                <span className="text-sm text-muted-foreground">
                  Loading folders...
                </span>
              </div>
            ) : (
              <ScrollArea className="h-80 rounded-md border">
                {browserData?.directories.length ? (
                  <div className="divide-y">
                    {browserData.directories.map((directory) => (
                      <div
                        key={directory.path}
                        className="flex flex-col gap-3 px-4 py-3 md:flex-row md:items-center md:justify-between"
                      >
                        <button
                          type="button"
                          className="group min-w-0 flex-1 rounded-md border border-transparent px-3 py-2 text-left transition-colors hover:border-primary/30 hover:bg-primary/5 focus-visible:ring-2 focus-visible:ring-primary/60 focus-visible:outline-none"
                          onClick={() => setBrowserPath(directory.path)}
                        >
                          <div className="flex items-center gap-2">
                            <HugeiconsIcon icon={Folder02Icon} size={14} />
                            <p className="truncate text-sm font-medium">
                              {directory.name}
                            </p>
                            {directory.is_git_repository ? (
                              <Badge variant="success">Git repo</Badge>
                            ) : null}
                          </div>
                          <p className="mt-1 truncate font-mono text-xs text-muted-foreground">
                            {directory.path}
                          </p>
                        </button>

                        {directory.is_git_repository ? (
                          <Button
                            type="button"
                            size="sm"
                            onClick={() => applyLocalPath(directory.path, true)}
                          >
                            <HugeiconsIcon icon={Add01Icon} size={14} />
                            Use Repo
                          </Button>
                        ) : null}
                      </div>
                    ))}
                  </div>
                ) : (
                  <div className="p-6">
                    <p className="text-sm text-muted-foreground">
                      No subfolders found in this location.
                    </p>
                  </div>
                )}
              </ScrollArea>
            )}
          </div>

          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={() => setPickerOpen(false)}
            >
              Done
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  )
}
