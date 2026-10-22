import { StepStatusIcon } from './step-status-icon'
import type { StepGroup } from './types'
import { HugeiconsIcon } from '@hugeicons/react'
import { FileTerminalIcon } from '@hugeicons/core-free-icons'
import {
  Item,
  ItemActions,
  ItemContent,
  ItemDescription,
  ItemGroup,
  ItemMedia,
  ItemTitle,
} from '@/components/ui/item'
import { ScrollArea } from '@/components/ui/scroll-area'
import { formatDuration } from '@/lib/format-utils'

interface StepNavigationProps {
  groups: Array<StepGroup>
  selectedStep: string
  allLogCount: number
  onSelect: (step: string) => void
}

export function StepNavigation({
  groups,
  selectedStep,
  allLogCount,
  onSelect,
}: StepNavigationProps) {
  return (
    <nav
      aria-label="Build steps"
      className="flex w-60 shrink-0 flex-col border-r bg-muted/10"
    >
      <div className="flex shrink-0 items-center justify-between border-b px-3 py-2.5">
        <span className="text-xs font-medium">Steps</span>
        <span className="text-xs text-muted-foreground tabular-nums">
          {groups.length === 1 ? '1 step' : `${groups.length} steps`}
        </span>
      </div>
      <ScrollArea className="min-h-0 flex-1">
        <ItemGroup className="gap-1 p-2">
          <StepItem
            selected={selectedStep === 'all'}
            onClick={() => onSelect('all')}
            name="Full log"
            lineCount={allLogCount}
          />
          {groups.map((group) => (
            <StepItem
              key={group.name}
              selected={selectedStep === group.name}
              onClick={() => onSelect(group.name)}
              group={group}
            />
          ))}
        </ItemGroup>
      </ScrollArea>
    </nav>
  )
}

function StepItem({
  selected,
  onClick,
  name,
  lineCount,
  group,
}: {
  selected: boolean
  onClick: () => void
  name?: string
  lineCount?: number
  group?: StepGroup
}) {
  const label = group?.name ?? name ?? ''
  const count = group?.logs.length ?? lineCount ?? 0

  return (
    <Item
      render={<button type="button" />}
      variant={selected ? 'muted' : 'default'}
      size="xs"
      onClick={onClick}
      className="shrink-0 cursor-pointer text-left shadow-[inset_2px_0_0_transparent] hover:bg-muted/50 aria-current:bg-muted aria-current:shadow-[inset_2px_0_0_var(--primary)]"
      aria-current={selected ? 'true' : undefined}
    >
      <ItemMedia variant="icon">
        {group ? (
          <StepStatusIcon status={group.status} />
        ) : (
          <HugeiconsIcon
            icon={FileTerminalIcon}
            className="text-muted-foreground"
          />
        )}
      </ItemMedia>
      <ItemContent className="min-w-0">
        <ItemTitle className="text-xs break-words">{label}</ItemTitle>
        <ItemDescription className="font-mono text-[10px]">
          {formatLineCount(count)}
        </ItemDescription>
      </ItemContent>
      {group?.durationMs != null ? (
        <ItemActions className="font-mono text-[10px] text-muted-foreground">
          {formatDuration(group.durationMs / 1000)}
        </ItemActions>
      ) : null}
    </Item>
  )
}

function formatLineCount(count: number) {
  return `${count} ${count === 1 ? 'line' : 'lines'}`
}
