import { formatDuration } from './log-model'
import { StepStatusIcon } from './step-status-icon'
import type { StepGroup } from './types'
import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'

interface StepNavigationProps {
  groups: Array<StepGroup>
  selectedStep: string
  allLogCount: number
  onSelect: (step: string) => void
}

export function StepNavigation(props: StepNavigationProps) {
  return (
    <nav
      aria-label="Build steps"
      className="shrink-0 border-b bg-muted/10 md:flex md:w-56 md:flex-col md:border-r md:border-b-0"
    >
      <div className="hidden shrink-0 items-center justify-between border-b px-3 py-2 md:flex">
        <span className="text-xs font-medium">Steps</span>
        <span className="font-mono text-[10px] text-muted-foreground">
          {props.groups.length}
        </span>
      </div>
      <div
        role="tablist"
        className="flex gap-1 overflow-x-auto p-2 md:min-h-0 md:flex-1 md:flex-col md:overflow-x-hidden md:overflow-y-auto"
      >
        <StepButton
          selected={props.selectedStep === 'all'}
          onClick={() => props.onSelect('all')}
          name="Full log"
          lineCount={props.allLogCount}
        />
        {props.groups.map((group) => (
          <StepButton
            key={group.name}
            selected={props.selectedStep === group.name}
            onClick={() => props.onSelect(group.name)}
            group={group}
          />
        ))}
      </div>
    </nav>
  )
}

function StepButton({
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
    <Button
      variant="ghost"
      size="xs"
      onClick={onClick}
      className={cn(
        'h-9 shrink-0 justify-start rounded-none border-l-2 px-2.5 text-muted-foreground md:h-auto md:min-h-11 md:w-full md:py-2',
        selected
          ? 'border-primary bg-accent text-foreground hover:bg-accent'
          : 'border-transparent hover:text-foreground',
      )}
      title={group?.command}
      role="tab"
      aria-selected={selected}
    >
      {group ? (
        <StepStatusIcon status={group.status} />
      ) : (
        <span className="size-4 shrink-0" aria-hidden />
      )}
      <span className="min-w-0 text-left md:flex-1">
        <span className="block max-w-40 truncate text-xs font-medium">
          {label}
        </span>
        <span className="hidden font-mono text-[10px] font-normal text-muted-foreground md:block">
          {count} {count === 1 ? 'line' : 'lines'}
        </span>
      </span>
      {group?.durationMs != null ? (
        <span className="ml-2 shrink-0 font-mono text-[10px] font-normal text-muted-foreground">
          {formatDuration(group.durationMs / 1000)}
        </span>
      ) : null}
    </Button>
  )
}
