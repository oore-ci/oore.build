import { HugeiconsIcon } from '@hugeicons/react'
import { Add01Icon, Cancel01Icon } from '@hugeicons/core-free-icons'
import { useMemo, useState } from 'react'
import type { ClipboardEvent, KeyboardEvent } from 'react'

import type { ProjectMemberCandidate, ProjectRole } from '@oore/client/models'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
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
  DialogTrigger,
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
import { useInviteUser } from '@/hooks/use-auth'
import { useHasPermission } from '@/hooks/use-permissions'
import {
  useGrantProjectAccess,
  useProjectMemberCandidates,
  useProjectMembers,
} from '@/hooks/use-projects'
import {
  PROJECT_ROLE_LABELS,
  PROJECT_ROLE_OPTIONS,
  mapWithLimit,
  parseEmailList,
  people,
  plural,
  projectRoleFor,
} from '@/lib/project-access'
import { textVariants } from '@/lib/recipes'
import { toast } from '@/lib/toast'

function instanceRoleLabel(role: string) {
  return role === 'qa_viewer' ? 'QA Viewer' : 'Developer'
}

interface Failure {
  email: string
  message: string
}

function candidateLabel(user: ProjectMemberCandidate) {
  return user.display_name ? `${user.display_name} (${user.email})` : user.email
}

export function AddProjectMembersDialog({ projectId }: { projectId: string }) {
  const candidatesQuery = useProjectMemberCandidates(projectId)
  const membersQuery = useProjectMembers(projectId)
  const grantMutation = useGrantProjectAccess()
  const inviteMutation = useInviteUser()
  const canInvite = useHasPermission('users:invite')
  const anchor = useComboboxAnchor()

  const [open, setOpen] = useState(false)
  const [selected, setSelected] = useState<Array<ProjectMemberCandidate>>([])
  const [newEmails, setNewEmails] = useState<Array<string>>([])
  const [alreadyMembers, setAlreadyMembers] = useState<Array<string>>([])
  const [role, setRole] = useState<ProjectRole>('developer')
  const [inputValue, setInputValue] = useState('')
  const [failures, setFailures] = useState<Array<Failure>>([])
  const [submitting, setSubmitting] = useState(false)

  const candidates = candidatesQuery.data?.candidates ?? []
  const candidatesByEmail = useMemo(
    () =>
      new Map(
        (candidatesQuery.data?.candidates ?? []).map((candidate) => [
          candidate.email.toLowerCase(),
          candidate,
        ]),
      ),
    [candidatesQuery.data?.candidates],
  )
  const memberEmails = useMemo(
    () =>
      new Set(
        (membersQuery.data?.members ?? []).map((member) =>
          member.user_email.toLowerCase(),
        ),
      ),
    [membersQuery.data?.members],
  )
  const unselectedCount = candidates.length - selected.length
  const total = selected.length + newEmails.length
  const hasQaViewers = selected.some((user) => user.role === 'qa_viewer')

  function reset() {
    setSelected([])
    setNewEmails([])
    setAlreadyMembers([])
    setRole('developer')
    setInputValue('')
    setFailures([])
  }

  function setDialogOpen(nextOpen: boolean) {
    if (submitting) return
    setOpen(nextOpen)
    if (!nextOpen) reset()
  }

  /** Sorts pasted or typed emails into people to add, invite, or skip. */
  function addEmails(emails: Array<string>) {
    const selectedIds = new Set(selected.map((user) => user.id))
    const toSelect: Array<ProjectMemberCandidate> = []
    const toInvite: Array<string> = []
    const members: Array<string> = []
    for (const email of emails) {
      const candidate = candidatesByEmail.get(email)
      if (candidate) {
        if (!selectedIds.has(candidate.id)) {
          selectedIds.add(candidate.id)
          toSelect.push(candidate)
        }
      } else if (memberEmails.has(email)) {
        members.push(email)
      } else {
        toInvite.push(email)
      }
    }
    setSelected((current) => [...current, ...toSelect])
    setNewEmails((current) => [
      ...current,
      ...toInvite.filter((email) => !current.includes(email)),
    ])
    setAlreadyMembers((current) => [
      ...current,
      ...members.filter((email) => !current.includes(email)),
    ])
    setFailures([])
  }

  function handlePaste(event: ClipboardEvent<HTMLInputElement>) {
    const emails = parseEmailList(event.clipboardData.getData('text'))
    if (emails.length === 0) return
    event.preventDefault()
    addEmails(emails)
    setInputValue('')
  }

  function handleKeyDown(event: KeyboardEvent<HTMLInputElement>) {
    if (event.key !== 'Enter' && event.key !== ',' && event.key !== ' ') return
    const emails = parseEmailList(inputValue)
    if (emails.length === 0) return
    event.preventDefault()
    event.stopPropagation()
    addEmails(emails)
    setInputValue('')
  }

  async function submit() {
    if (total === 0) return
    setSubmitting(true)
    setFailures([])
    const nextFailures: Array<Failure> = []

    // People without an Oore account are invited first, then added like
    // everyone else.
    const invited: Array<{ email: string; id: string; role: string }> = []
    const inviteResults = await mapWithLimit(newEmails, 4, (email) =>
      inviteMutation.mutateAsync({ email, role: 'developer' }),
    )
    const stillToInvite: Array<string> = []
    inviteResults.forEach((result, index) => {
      const email = newEmails[index]
      if (result.status === 'fulfilled' && result.value) {
        const user = result.value.user
        invited.push({ email, id: user.id, role: user.role })
      } else {
        stillToInvite.push(email)
        const reason: unknown =
          result.status === 'rejected' ? result.reason : undefined
        nextFailures.push({
          email,
          message:
            reason instanceof Error ? reason.message : 'The invite failed',
        })
      }
    })

    const targets = [
      ...selected.map((user) => ({
        email: user.email,
        id: user.id,
        role: user.role,
      })),
      ...invited,
    ]
    const outcomes = await grantMutation.mutateAsync(
      targets.map((person) => ({
        projectId,
        userId: person.id,
        role: projectRoleFor(person.role, role),
      })),
    )
    const failedIds = new Set<string>()
    let added = 0
    outcomes.forEach((outcome, index) => {
      if (outcome.status === 'failed') {
        failedIds.add(outcome.grant.userId)
        nextFailures.push({
          email: targets[index].email,
          message: outcome.message,
        })
      } else {
        added++
      }
    })
    setSubmitting(false)

    if (nextFailures.length === 0) {
      toast.success(`${people(added)} added to this project`)
      setOpen(false)
      reset()
      return
    }

    // Keep only what failed so a retry does not repeat finished work.
    setSelected((current) => current.filter((user) => failedIds.has(user.id)))
    setNewEmails(stillToInvite)
    setFailures(nextFailures)
    if (added > 0) {
      toast.success(`${people(added)} added`)
    }
  }

  const submitLabel = total === 0 ? 'Add people' : `Add ${people(total)}`

  return (
    <Dialog open={open} onOpenChange={setDialogOpen}>
      <DialogTrigger render={<Button />}>
        <HugeiconsIcon icon={Add01Icon} data-icon="inline-start" />
        Add members
      </DialogTrigger>
      <DialogContent className="sm:max-w-xl">
        <DialogHeader>
          <DialogTitle>Add project members</DialogTitle>
          <DialogDescription>
            Pick people or paste a list of emails. Everyone gets the same
            project role.
          </DialogDescription>
        </DialogHeader>

        <form
          className="space-y-4"
          onSubmit={(event) => {
            event.preventDefault()
            void submit()
          }}
        >
          <div className="space-y-2">
            <div className="flex items-center justify-between gap-2">
              <Label htmlFor="project-members-input">People</Label>
              {unselectedCount > 0 ? (
                <Button
                  type="button"
                  variant="link"
                  size="sm"
                  className="h-auto p-0"
                  onClick={() => {
                    setSelected(candidates)
                    setFailures([])
                  }}
                >
                  Select all {candidates.length} eligible
                </Button>
              ) : null}
            </div>
            <Combobox
              multiple
              autoHighlight
              items={candidates}
              value={selected}
              onValueChange={(value: Array<ProjectMemberCandidate>) => {
                setSelected(value)
                setFailures([])
              }}
              inputValue={inputValue}
              onInputValueChange={setInputValue}
              itemToStringLabel={candidateLabel}
              isItemEqualToValue={(item, value) => item.id === value.id}
            >
              <ComboboxChips ref={anchor} className="max-h-48 overflow-y-auto">
                <ComboboxValue>
                  {(values: Array<ProjectMemberCandidate>) => (
                    <>
                      {values.map((user) => (
                        <ComboboxChip key={user.id}>
                          {user.display_name ?? user.email}
                        </ComboboxChip>
                      ))}
                      {newEmails.map((email) => (
                        <Badge
                          key={email}
                          variant="outline"
                          className="gap-1 border-dashed pr-0.5"
                        >
                          {email}
                          <span
                            className={textVariants({ variant: 'caption' })}
                          >
                            new
                          </span>
                          <Button
                            type="button"
                            variant="ghost"
                            size="icon-xs"
                            className="size-4.5 opacity-50 hover:opacity-100"
                            aria-label={`Remove ${email}`}
                            onClick={() =>
                              setNewEmails((current) =>
                                current.filter((value) => value !== email),
                              )
                            }
                          >
                            <HugeiconsIcon
                              icon={Cancel01Icon}
                              strokeWidth={2}
                              className="pointer-events-none"
                            />
                          </Button>
                        </Badge>
                      ))}
                      <ComboboxChipsInput
                        id="project-members-input"
                        placeholder={
                          total === 0
                            ? 'Search people, or paste a list of emails'
                            : ''
                        }
                        onPaste={handlePaste}
                        onKeyDown={handleKeyDown}
                      />
                    </>
                  )}
                </ComboboxValue>
              </ComboboxChips>
              <ComboboxContent anchor={anchor}>
                <ComboboxEmpty>
                  {inputValue.includes('@')
                    ? 'Press Enter to add this email.'
                    : 'No eligible users found.'}
                </ComboboxEmpty>
                <ComboboxList>
                  {(user: ProjectMemberCandidate) => (
                    <ComboboxItem key={user.id} value={user}>
                      <span className="min-w-0 flex-1 truncate">
                        {user.display_name ? (
                          <>
                            {user.display_name}{' '}
                            <span
                              className={textVariants({ variant: 'caption' })}
                            >
                              {user.email}
                            </span>
                          </>
                        ) : (
                          user.email
                        )}
                      </span>
                      <span className={textVariants({ variant: 'caption' })}>
                        {instanceRoleLabel(user.role)}
                        {user.status === 'invited' ? ' · Invited' : ''}
                      </span>
                    </ComboboxItem>
                  )}
                </ComboboxList>
              </ComboboxContent>
            </Combobox>
            {newEmails.length > 0 ? (
              <p className={textVariants({ variant: 'caption' })}>
                {canInvite
                  ? `${plural(newEmails.length, 'email')} ${newEmails.length === 1 ? "isn't" : "aren't"} in Oore yet. They will be invited as Developers and added here.`
                  : `${plural(newEmails.length, 'email')} ${newEmails.length === 1 ? "isn't" : "aren't"} in Oore yet. Ask an admin to invite them first.`}
              </p>
            ) : null}
            {alreadyMembers.length > 0 ? (
              <p className={textVariants({ variant: 'caption' })}>
                Already on this project: {alreadyMembers.join(', ')}
              </p>
            ) : null}
          </div>

          <div className="space-y-2">
            <Label htmlFor="project-members-role">Project role</Label>
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
              <SelectTrigger id="project-members-role" className="w-full">
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
            {hasQaViewers && role !== 'viewer' ? (
              <p className={textVariants({ variant: 'caption' })}>
                QA viewers always get read-only Viewer access.
              </p>
            ) : null}
          </div>

          {candidatesQuery.error ? (
            <Alert variant="destructive">
              <AlertDescription>
                Failed to load eligible users: {candidatesQuery.error.message}
              </AlertDescription>
            </Alert>
          ) : null}

          {failures.length > 0 ? (
            <Alert variant="destructive">
              <AlertDescription>
                <p>
                  {people(failures.length)} could not be added. They are still
                  selected so you can retry.
                </p>
                <ul className="mt-1 list-disc pl-4">
                  {failures.map((failure) => (
                    <li key={failure.email}>
                      {failure.email}: {failure.message}
                    </li>
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
              disabled={submitting}
            >
              Cancel
            </Button>
            <Button
              type="submit"
              disabled={
                submitting ||
                total === 0 ||
                candidatesQuery.isLoading ||
                (newEmails.length > 0 && !canInvite)
              }
            >
              {submitting ? (
                <>
                  <Spinner className="size-4" />
                  Adding...
                </>
              ) : (
                submitLabel
              )}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}
