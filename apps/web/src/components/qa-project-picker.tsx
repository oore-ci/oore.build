import type { Project } from '@/lib/types'
import RepositoryAvatar from '@/components/repository-avatar'
import { Button } from '@/components/ui/button'
import {
  Combobox,
  ComboboxContent,
  ComboboxEmpty,
  ComboboxInput,
  ComboboxItem,
  ComboboxList,
} from '@/components/ui/combobox'
import { InputGroupAddon } from '@/components/ui/input-group'

export default function QaProjectPicker({
  hasMoreProjects,
  isFetchingMoreProjects,
  onLoadMoreProjects,
  onOpenChange,
  onProjectChange,
  open,
  project,
  projects,
}: {
  hasMoreProjects: boolean
  isFetchingMoreProjects: boolean
  onLoadMoreProjects: () => void
  onOpenChange: (open: boolean) => void
  onProjectChange: (projectId: string) => void
  open: boolean
  project: Project
  projects: Array<Project>
}) {
  return (
    <Combobox
      items={projects}
      value={project}
      open={open}
      onOpenChange={onOpenChange}
      onValueChange={(nextProject) => {
        if (nextProject) onProjectChange(nextProject.id)
      }}
      itemToStringLabel={(item) => item.name}
    >
      <ComboboxInput
        autoFocus
        className="w-full"
        placeholder="Choose an app"
        aria-label="Choose an app"
      >
        <InputGroupAddon align="inline-start">
          <RepositoryAvatar
            fullName={project.repository_full_name ?? project.name}
            avatarUrl={project.repository_avatar_url}
            repositoryId={project.repository_id}
            provider={project.repository_provider}
            size="sm"
          />
        </InputGroupAddon>
      </ComboboxInput>
      <ComboboxContent>
        <ComboboxEmpty>No matching apps.</ComboboxEmpty>
        <ComboboxList>
          {(item) => (
            <ComboboxItem key={item.id} value={item}>
              <RepositoryAvatar
                fullName={item.repository_full_name ?? item.name}
                avatarUrl={item.repository_avatar_url}
                repositoryId={item.repository_id}
                provider={item.repository_provider}
                size="sm"
              />
              <span className="truncate">{item.name}</span>
            </ComboboxItem>
          )}
        </ComboboxList>
        {hasMoreProjects ? (
          <div className="border-t p-2">
            <Button
              variant="ghost"
              size="sm"
              className="w-full"
              disabled={isFetchingMoreProjects}
              onClick={onLoadMoreProjects}
            >
              {isFetchingMoreProjects ? 'Loading more…' : 'Load more apps'}
            </Button>
          </div>
        ) : null}
      </ComboboxContent>
    </Combobox>
  )
}
