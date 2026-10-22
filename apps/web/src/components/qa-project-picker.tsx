import { useRef } from 'react'

import type { Project } from '@oore/client/models'
import QaAppMonogram from '@/components/qa-app-monogram'
import { Button } from '@/components/ui/button'
import {
  Combobox,
  ComboboxContent,
  ComboboxEmpty,
  ComboboxInput,
  ComboboxItem,
  ComboboxList,
  ComboboxTrigger,
} from '@/components/ui/combobox'
import { isNearScrollEnd } from '@/lib/scroll'

export default function QaProjectPicker({
  hasMoreProjects,
  isFetchingMoreProjects,
  onLoadMoreProjects,
  onOpenChange,
  onProjectChange,
  onSearchProjects,
  open,
  project,
  projects,
}: {
  hasMoreProjects: boolean
  isFetchingMoreProjects: boolean
  onLoadMoreProjects: () => void
  onOpenChange: (open: boolean) => void
  onProjectChange: (projectId: string) => void
  onSearchProjects: (search: string) => void
  open: boolean
  project: Project
  projects: Array<Project>
}) {
  const triggerRef = useRef<HTMLButtonElement>(null)

  return (
    <Combobox
      items={projects}
      filter={null}
      value={project}
      open={open}
      onOpenChange={onOpenChange}
      onValueChange={(nextProject) => {
        if (nextProject) onProjectChange(nextProject.id)
      }}
      onInputValueChange={(value, details) => {
        if (
          details.reason === 'input-change' ||
          details.reason === 'input-clear'
        ) {
          onSearchProjects(value)
        }
      }}
      isItemEqualToValue={(item, value) => item.id === value.id}
      itemToStringLabel={(item) => item.name}
    >
      <ComboboxTrigger
        ref={triggerRef}
        render={
          <Button
            variant="ghost"
            size="sm"
            className="max-w-48 min-w-0 justify-between px-2 font-normal"
            aria-label={`Choose app, currently ${project.name}`}
          />
        }
      >
        <span className="flex min-w-0 items-center gap-2">
          <QaAppMonogram name={project.name} />
          <span className="truncate">{project.name}</span>
        </span>
      </ComboboxTrigger>
      <ComboboxContent
        anchor={triggerRef}
        className="w-72 max-w-[calc(100vw-2rem)] bg-popover"
      >
        <ComboboxInput
          autoFocus
          className="w-auto"
          placeholder="Search apps"
          aria-label="Search apps"
        />
        <ComboboxEmpty>No matching apps.</ComboboxEmpty>
        <ComboboxList
          onScroll={(event) => {
            const list = event.currentTarget
            if (
              isNearScrollEnd(list) &&
              hasMoreProjects &&
              !isFetchingMoreProjects
            ) {
              onLoadMoreProjects()
            }
          }}
        >
          {(item) => (
            <ComboboxItem key={item.id} value={item}>
              <QaAppMonogram name={item.name} />
              <span className="truncate">{item.name}</span>
            </ComboboxItem>
          )}
        </ComboboxList>
      </ComboboxContent>
    </Combobox>
  )
}
