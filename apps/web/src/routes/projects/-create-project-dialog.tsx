import { useMemo, useState } from 'react'
import { useForm } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import { z } from 'zod'
import { useNavigate } from '@tanstack/react-router'
import { toast } from 'sonner'

import { useQuery } from '@tanstack/react-query'
import type { IntegrationRepository } from '@/lib/types'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import {
  Form,
  FormControl,
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
import { useCreateProject } from '@/hooks/use-projects'
import { listIntegrationRepos, listIntegrations } from '@/lib/api'
import { useActiveInstance } from '@/stores/instance-store'
import { useAuthStore } from '@/stores/auth-store'

const createProjectSchema = z.object({
  name: z.string().min(1, 'Name is required'),
  description: z.string().optional(),
  default_branch: z.string().optional(),
})

type CreateProjectForm = z.infer<typeof createProjectSchema>

interface CreateProjectDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
}

function useAvailableRepos() {
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
          // skip failed integrations
        }
      }
      return repos
    },
    enabled: !!baseUrl && !!token,
  })
}

export default function CreateProjectDialog({
  open,
  onOpenChange,
}: CreateProjectDialogProps) {
  const navigate = useNavigate()
  const createMutation = useCreateProject()
  const { data: repos, isLoading: reposLoading } = useAvailableRepos()
  const [selectedRepoId, setSelectedRepoId] = useState<string>('')
  const repoItems = useMemo(
    () => Object.fromEntries((repos ?? []).map((r) => [r.id, r.full_name])),
    [repos],
  )

  const form = useForm<CreateProjectForm>({
    resolver: zodResolver(createProjectSchema),
    defaultValues: { name: '', description: '', default_branch: '' },
    mode: 'onBlur',
  })

  function onSubmit(data: CreateProjectForm) {
    createMutation.mutate(
      {
        name: data.name.trim(),
        description: data.description?.trim() || undefined,
        repository_id: selectedRepoId || undefined,
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
    }
    onOpenChange(nextOpen)
  }

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Create Project</DialogTitle>
          <DialogDescription>
            Create a new CI project. Optionally link it to a repository.
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

            <div className="space-y-2">
              <FormLabel>
                Repository{' '}
                <span className="text-muted-foreground font-normal">
                  (optional)
                </span>
              </FormLabel>
              {reposLoading ? (
                <div className="flex items-center gap-2 py-2">
                  <Spinner className="size-4" />
                  <span className="text-sm text-muted-foreground">
                    Loading repositories...
                  </span>
                </div>
              ) : repos && repos.length > 0 ? (
                <Select
                  value={selectedRepoId}
                  onValueChange={(v) => setSelectedRepoId(v ?? '')}
                  items={repoItems}
                >
                  <SelectTrigger>
                    <SelectValue placeholder="Select a repository..." />
                  </SelectTrigger>
                  <SelectContent>
                    {repos.map((repo) => (
                      <SelectItem key={repo.id} value={repo.id}>
                        {repo.full_name}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              ) : (
                <p className="text-sm text-muted-foreground">
                  No repositories available. Add an integration first.
                </p>
              )}
            </div>

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
              <Button type="submit" disabled={createMutation.isPending}>
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
  )
}
