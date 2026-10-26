import { useState } from 'react'

import type { Project, ProjectRole, User } from '@oore/client/models'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import {
  Combobox,
  ComboboxChip,
  ComboboxChips,
  ComboboxChipsInput,
  ComboboxContent,
  ComboboxEmpty,
  ComboboxItem,
  ComboboxList,
  ComboboxValue,
  useComboboxAnchor,
} from '@/components/ui/combobox'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Label } from '@/components/ui/label'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Spinner } from '@/components/ui/spinner'
import { useGrantProjectAccess, useProjects } from '@/hooks/use-projects'
import {
  PROJECT_ROLE_LABELS,
  PROJECT_ROLE_OPTIONS,
  hasImplicitProjectAccess,
  people,
  plural,
  projectRoleFor,
} from '@/lib/project-access'
import { textVariants } from '@/lib/recipes'
import { toast } from '@/lib/toast'

interface Failure {
  key: string
  text: string
}

export function GrantProjectAccessDialog({
  onDone,
  onOpenChange,
  open,
  users,
}: {
  onDone: () => void
  onOpenChange: (open: boolean) => void
  open: boolean
  users: Array<User>
}) {
  const projectsQuery = useProjects({ limit: 200 }, { enabled: open })
  const grantMutation = useGrantProjectAccess()
  const anchor = useComboboxAnchor()
  const [projects, setProjects] = useState<Array<Project>>([])
  const [role, setRole] = useState<ProjectRole>('developer')
  const [failures, setFailures] = useState<Array<Failure>>([])

  const allProjects = projectsQuery.data?.projects ?? []
  const eligible = users.filter(
    (user) =>
      user.status !== 'disabled' && !hasImplicitProjectAccess(user.role),
  )
  const implicit = users.filter((user) => hasImplicitProjectAccess(user.role))
  const disabled = users.filter((user) => user.status === 'disabled')
  const hasQaViewers = eligible.some((user) => user.role === 'qa_viewer')
  const grantCount = eligible.length * projects.length
  const usersById = new Map(users.map((user) => [user.id, user]))
  const projectsById = new Map(
    allProjects.map((project) => [project.id, project]),
  )

  function setDialogOpen(nextOpen: boolean) {
    if (grantMutation.isPending) return
    onOpenChange(nextOpen)
    if (!nextOpen) {
      setProjects([])
      setRole('developer')
      setFailures([])
    }
  }

  async function submit() {
    if (grantCount === 0) return
    setFailures([])
    const outcomes = await grantMutation.mutateAsync(
      eligible.flatMap((user) =>
        projects.map((project) => ({
          projectId: project.id,
          userId: user.id,
          role: projectRoleFor(user.role, role),
        })),
      ),
    )
    const added = outcomes.filter((outcome) => outcome.status === 'added')
    const existing = outcomes.filter(
      (outcome) => outcome.status === 'already_member',
    )
    const failed = outcomes.flatMap((outcome) =>
      outcome.status === 'failed'
        ? [
            {
              key: `${outcome.grant.userId}:${outcome.grant.projectId}`,
              text: `${usersById.get(outcome.grant.userId)?.email ?? 'User'} on ${projectsById.get(outcome.grant.projectId)?.name ?? 'project'}: ${outcome.message}`,
            },
          ]
        : [],
    )
    const summary = [
      added.length > 0
        ? `Added ${plural(added.length, 'project membership')}`
        : null,
      existing.length > 0
        ? `${existing.length} already existed (roles unchanged)`
        : null,
    ]
      .filter(Boolean)
      .join('; ')

    if (failed.length === 0) {
      toast.success(summary || 'Access updated')
      setDialogOpen(false)
      onDone()
      return
    }
    if (summary) toast.success(summary)
    setFailures(failed)
  }

  return (
    <Dialog open={open} onOpenChange={setDialogOpen}>
      <DialogContent className="sm:max-w-xl">
        <DialogHeader>
          <DialogTitle>Give project access</DialogTitle>
          <DialogDescription>
            {eligible.length === 1
              ? `Add ${eligible[0].email} to one or more projects.`
              : `Add ${people(eligible.length)} to one or more projects with the same role.`}
          </DialogDescription>
        </DialogHeader>

        <form
          className="space-y-4"
          onSubmit={(event) => {
            event.preventDefault()
            void submit()
          }}
        >
          {implicit.length > 0 || disabled.length > 0 ? (
            <p className={textVariants({ variant: 'caption' })}>
              {[
                implicit.length > 0
                  ? `Skipping ${implicit.map((user) => user.email).join(', ')}: owners and admins already see every project.`
                  : null,
                disabled.length > 0
                  ? `Skipping ${disabled.map((user) => user.email).join(', ')}: disabled users can't be added.`
                  : null,
              ]
                .filter(Boolean)
                .join(' ')}
            </p>
          ) : null}

          <div className="space-y-2">
            <div className="flex items-center justify-between gap-2">
              <Label htmlFor="grant-projects-input">Projects</Label>
              {allProjects.length > 0 &&
              projects.length < allProjects.length ? (
                <Button
                  type="button"
                  variant="link"
                  size="sm"
                  className="h-auto p-0"
                  onClick={() => {
                    setProjects(allProjects)
                    setFailures([])
                  }}
                >
                  Select all {allProjects.length}
                </Button>
              ) : null}
            </div>
            <Combobox
              multiple
              autoHighlight
              items={allProjects}
              value={projects}
              onValueChange={(value: Array<Project>) => {
                setProjects(value)
                setFailures([])
              }}
              itemToStringLabel={(project: Project) => project.name}
              isItemEqualToValue={(item, value) => item.id === value.id}
            >
              <ComboboxChips ref={anchor} className="max-h-48 overflow-y-auto">
                <ComboboxValue>
                  {(values: Array<Project>) => (
                    <>
                      {values.map((project) => (
                        <ComboboxChip key={project.id}>
                          {project.name}
                        </ComboboxChip>
                      ))}
                      <ComboboxChipsInput
                        id="grant-projects-input"
                        placeholder={
                          values.length === 0 ? 'Search projects' : ''
                        }
                      />
                    </>
                  )}
                </ComboboxValue>
              </ComboboxChips>
              <ComboboxContent anchor={anchor}>
                <ComboboxEmpty>
                  {projectsQuery.isLoading
                    ? 'Loading projects…'
                    : 'No projects found.'}
                </ComboboxEmpty>
                <ComboboxList>
                  {(project: Project) => (
                    <ComboboxItem key={project.id} value={project}>
                      <span className="min-w-0 flex-1 truncate">
                        {project.name}
                      </span>
                    </ComboboxItem>
                  )}
                </ComboboxList>
              </ComboboxContent>
            </Combobox>
          </div>

          <div className="space-y-2">
            <Label htmlFor="grant-projects-role">Project role</Label>
            <Select
              value={role}
              onValueChange={(value) => {
                const next = PROJECT_ROLE_OPTIONS.find(
                  (option) => option === value,
                )
                if (next) setRole(next)
              }}
              items={PROJECT_ROLE_LABELS}
            >
              <SelectTrigger id="grant-projects-role" className="w-full">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {PROJECT_ROLE_OPTIONS.map((option) => (
                  <SelectItem key={option} value={option}>
                    {PROJECT_ROLE_LABELS[option]}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            <p className={textVariants({ variant: 'caption' })}>
              People who are already on a project keep their current role.
              {hasQaViewers && role !== 'viewer'
                ? ' QA viewers always get read-only Viewer access.'
                : ''}
            </p>
          </div>

          {projectsQuery.error ? (
            <Alert variant="destructive">
              <AlertDescription>
                Failed to load projects: {projectsQuery.error.message}
              </AlertDescription>
            </Alert>
          ) : null}

          {failures.length > 0 ? (
            <Alert variant="destructive">
              <AlertDescription>
                <p>
                  {plural(failures.length, 'grant')} failed. Submit again to
                  retry; finished ones are skipped.
                </p>
                <ul className="mt-1 list-disc pl-4">
                  {failures.map((failure) => (
                    <li key={failure.key}>{failure.text}</li>
                  ))}
                </ul>
              </AlertDescription>
            </Alert>
          ) : null}

          <DialogFooter className="static">
            <Button
              type="button"
              variant="outline"
              onClick={() => setDialogOpen(false)}
              disabled={grantMutation.isPending}
            >
              Cancel
            </Button>
            <Button
              type="submit"
              disabled={grantMutation.isPending || grantCount === 0}
            >
              {grantMutation.isPending ? (
                <>
                  <Spinner className="size-4" />
                  Adding...
                </>
              ) : projects.length > 0 ? (
                `Add to ${plural(projects.length, 'project')}`
              ) : (
                'Give access'
              )}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}
