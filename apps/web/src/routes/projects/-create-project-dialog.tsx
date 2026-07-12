import { useMemo, useState } from 'react'
import { useForm } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import z from 'zod'
import { Link, useNavigate } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import { Folder02Icon } from '@hugeicons/core-free-icons'
import { toast } from 'sonner'

import { useQuery } from '@tanstack/react-query'
import type { IntegrationRepository, ScmProvider } from '@/lib/types'
import { Button } from '@/components/ui/button'
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
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { useCreateProject } from '@/hooks/use-projects'
import { useSetupStatus } from '@/hooks/use-setup'
import { listIntegrationRepos, listIntegrations } from '@/lib/api'
import { useActiveInstance } from '@/stores/instance-store'
import { useAuthStore } from '@/stores/auth-store'
import { resolveInstanceApiBaseUrl } from '@/lib/instance-url'

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

type SourceRepository = IntegrationRepository & {
  integration_id: string
  provider: ScmProvider
  host_url: string
}

function sourceProviderLabel(provider: ScmProvider): string {
  if (provider === 'gitlab') return 'GitLab'
  if (provider === 'github') return 'GitHub'
  return 'Local Git'
}

function isLoopbackHostname(hostname: string): boolean {
  return (
    hostname === 'localhost' ||
    hostname === '127.0.0.1' ||
    hostname === '::1' ||
    hostname === '[::1]'
  )
}

function resolveHostname(rawUrl: string | null | undefined): string {
  const trimmed = rawUrl?.trim() ?? ''
  if (!trimmed) return ''
  try {
    return new URL(trimmed).hostname
  } catch {
    return ''
  }
}

function useAvailableRepos(enabled: boolean) {
  const instance = useActiveInstance()
  const token = useAuthStore((s) => s.token)
  const baseUrl = resolveInstanceApiBaseUrl(instance)

  return useQuery({
    queryKey: [instance?.id ?? '__none__', 'all-repos-for-project'],
    queryFn: async () => {
      if (!baseUrl || !token) return []
      const intResp = await listIntegrations(baseUrl, token)
      const repos: Array<SourceRepository> = []
      for (const integration of intResp.integrations) {
        try {
          const repoResp = await listIntegrationRepos(
            baseUrl,
            token,
            integration.id,
          )
          repos.push(
            ...repoResp.repositories.map((repository) => ({
              ...repository,
              integration_id: integration.id,
              provider: integration.provider,
              host_url: integration.host_url,
            })),
          )
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
  const runtimeMode = setupStatusQuery.data?.runtime_mode ?? 'local'
  const isRemoteMode = runtimeMode === 'remote'
  const instance = useActiveInstance()
  const instanceApiBaseUrl = resolveInstanceApiBaseUrl(instance)

  const uiIsLoopback = isLoopbackHostname(window.location.hostname)
  const backendIsLoopback = isLoopbackHostname(
    resolveHostname(instanceApiBaseUrl),
  )
  const canBrowseLocalFs = uiIsLoopback && backendIsLoopback

  const [sourceKind, setSourceKind] = useState<'local' | 'repo'>('local')
  const { data: repos, isLoading: reposLoading } = useAvailableRepos(
    open && isRemoteMode && sourceKind === 'repo',
  )
  const [selectedRepoId, setSelectedRepoId] = useState<string>('')
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
    },
    mode: 'onBlur',
  })

  const effectiveSourceKind = sourceKind
  const effectiveSelectedRepoId = selectedRepoId || repos?.[0]?.id || ''

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

    if (effectiveSourceKind === 'local') {
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

    if (!effectiveSelectedRepoId) {
      toast.error('Select a source repository before creating a project.')
      return
    }

    createMutation.mutate(
      {
        name,
        description: data.description?.trim() || undefined,
        repository_id: effectiveSelectedRepoId,
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
    if (nextOpen) {
      setSourceKind(isRemoteMode ? 'repo' : 'local')
      setSelectedRepoId('')
    } else {
      form.reset()
      setSelectedRepoId('')
      setSourceKind('local')
      setPickerOpen(false)
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
              {effectiveSourceKind === 'local'
                ? 'Create a project from a local repository path.'
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

              <Tabs
                value={effectiveSourceKind}
                onValueChange={(v) => {
                  setSourceKind(v as 'local' | 'repo')
                }}
              >
                <TabsList className="w-full">
                  <TabsTrigger value="local">Local path</TabsTrigger>
                  <TabsTrigger value="repo" disabled={!isRemoteMode}>
                    Connected repo
                  </TabsTrigger>
                </TabsList>

                <TabsContent value="local" className="space-y-4">
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
                                <HugeiconsIcon icon={Folder02Icon} size={16} />
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
                </TabsContent>

                <TabsContent value="repo" className="space-y-2">
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
                    <div className="space-y-3">
                      <p className="text-sm text-muted-foreground">
                        No source repositories are available yet. Connect a
                        source, then sync its repositories before creating a
                        project.
                      </p>
                      <Button
                        type="button"
                        variant="outline"
                        render={<Link to="/settings/integrations" />}
                        nativeButton={false}
                      >
                        Connect Source
                      </Button>
                    </div>
                  )}
                </TabsContent>
              </Tabs>

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
                    (effectiveSourceKind === 'repo' &&
                      (reposLoading || !hasRepos))
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

      <LocalFolderPickerDialog
        open={pickerOpen}
        onOpenChange={setPickerOpen}
        enabled={open && effectiveSourceKind === 'local' && canBrowseLocalFs}
        initialPath={form.getValues('local_repository_path')}
        title="Browse Local Folders"
        description="Select a Git repository folder and use it for this project."
        requireGitRepository
        selectCurrentLabel="Use Current Folder"
        selectDirectoryLabel="Use Repo"
        onSelectPath={(path) => applyLocalPath(path, true)}
      />
    </>
  )
}
