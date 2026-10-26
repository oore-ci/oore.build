import { StepStatusIcon } from './step-status-icon'
import { StepDurationBar, StepMedianDelta } from './step-timing'
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
import { longestStepMs } from '@/lib/step-timings'
import { cn } from '@/lib/utils'
import { textVariants } from '@/lib/recipes'

interface StepNavigationProps {
  groups: Array<StepGroup>
  medians?: ReadonlyMap<string, number>
  selectedStep: string
  allLogCount: number
  onSelect: (step: string) => void
}

export function StepNavigation({
  groups,
  medians,
  selectedStep,
  allLogCount,
  onSelect,
}: StepNavigationProps) {
  // Bars share one scale, so the slowest step or median fills the track.
  const longestMs = longestStepMs(groups, medians)
  return (
    <nav
      aria-label="Build steps"
      className="flex w-60 shrink-0 flex-col border-r bg-muted/10"
    >
      <div className="flex shrink-0 items-center justify-between border-b px-3 py-2.5">
        <span className="text-xs font-medium">Steps</span>
        <span
          className={cn(textVariants({ variant: 'caption' }), 'tabular-nums')}
        >
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
              longestMs={longestMs}
              medianMs={medians?.get(group.name)}
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
  longestMs = 0,
  medianMs,
}: {
  selected: boolean
  onClick: () => void
  name?: string
  lineCount?: number
  group?: StepGroup
  longestMs?: number
  medianMs?: number
}) {
  const durationMs = group?.durationMs
  const label = group?.name ?? name ?? ''
  const count = group?.logs.length ?? lineCount ?? 0

  return (
    <Item
      render={<button type="button" />}
      variant={selected ? 'muted' : 'default'}
      size="xs"
      onClick={onClick}
      className="shrink-0 cursor-pointer text-left hover:bg-muted/50 aria-current:bg-muted"
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
        <ItemTitle className="text-xs wrap-break-word">{label}</ItemTitle>
        <ItemDescription className="font-mono text-[10px]">
          {formatLineCount(count)}
        </ItemDescription>
        {durationMs != null && longestMs > 0 ? (
          <StepDurationBar
            durationMs={durationMs}
            longestMs={longestMs}
            medianMs={medianMs}
          />
        ) : null}
      </ItemContent>
      {durationMs != null ? (
        <ItemActions className="flex-col items-end gap-0.5 self-start font-mono text-[10px] text-muted-foreground">
          <span>{formatDuration(durationMs / 1000)}</span>
          {medianMs != null ? (
            <StepMedianDelta durationMs={durationMs} medianMs={medianMs} />
          ) : null}
        </ItemActions>
      ) : null}
    </Item>
  )
}

function formatLineCount(count: number) {
  return `${count} ${count === 1 ? 'line' : 'lines'}`
}
