import { useMemo, useState } from 'react'
import { useNavigate } from '@tanstack/react-router'
import { zodResolver } from '@hookform/resolvers/zod'
import { useForm } from 'react-hook-form'
import { toast } from 'sonner'
import * as z from 'zod'

import type { ScmProvider } from '@/lib/types'
import { useCreateProject } from '@/hooks/use-projects'
import { useSetupStatus } from '@/hooks/use-setup'
import { useSourceRepositories } from '@/hooks/use-source-repositories'
import { resolveInstanceApiBaseUrl } from '@/lib/instance-url'
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

export function useCreateProjectDialogState(
  open: boolean,
  onOpenChange: (open: boolean) => void,
) {
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

  const { data: repos, isLoading: reposLoading } = useSourceRepositories(
    open && isRemoteMode,
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

    if (!isRemoteMode) {
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

    const repositoryId = data.repository_id?.trim()
    if (!repositoryId) {
      toast.error('Select a source repository before creating a project.')
      return
    }

    createMutation.mutate(
      {
        name,
        description: data.description?.trim() || undefined,
        repository_id: repositoryId,
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
  }

  function handleOpenChange(nextOpen: boolean) {
    if (!nextOpen) {
      form.reset()
      setPickerOpen(false)
    }
    onOpenChange(nextOpen)
  }

  return {
    canBrowseLocalFs,
    createMutation,
    form,
    handleOpenChange,
    handleOpenPicker,
    hasRepos,
    isRemoteMode,
    onSubmit,
    open,
    pickerOpen,
    repoItems,
    repos,
    reposLoading,
    setPickerOpen,
  }
}
