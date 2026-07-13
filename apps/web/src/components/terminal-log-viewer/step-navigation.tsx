import { formatDuration } from './log-model'
import { StepStatusIcon } from './step-status-icon'
import type { StepGroup } from './types'
import { Button } from '@/components/ui/button'

interface StepNavigationProps {
  groups: Array<StepGroup>
  selectedStep: string
  allLogCount: number
  onSelect: (step: string) => void
}

export function StepNavigation(props: StepNavigationProps) {
  return (
    <div
      role="tablist"
      aria-label="Build steps"
      className="flex max-h-44 shrink-0 flex-col overflow-y-auto border-b bg-muted/10 p-1"
    >
      <Button
        variant={props.selectedStep === 'all' ? 'secondary' : 'ghost'}
        size="xs"
        onClick={() => props.onSelect('all')}
        className="h-auto w-full justify-start rounded-none px-3 py-2"
        role="tab"
        aria-selected={props.selectedStep === 'all'}
      >
        <span className="font-medium">All logs</span>
        <span className="ml-auto font-mono text-[10px] text-muted-foreground">
          {props.allLogCount}
        </span>
      </Button>
      {props.groups.map((group) => (
        <Button
          key={group.name}
          variant={props.selectedStep === group.name ? 'secondary' : 'ghost'}
          size="xs"
          onClick={() => props.onSelect(group.name)}
          className="h-auto w-full justify-start rounded-none px-3 py-2"
          title={group.command}
          role="tab"
          aria-selected={props.selectedStep === group.name}
        >
          <StepStatusIcon status={group.status} />
          <span className="min-w-0 flex-1 truncate">{group.name}</span>
          <span className="shrink-0 font-mono text-[10px] text-muted-foreground">
            {group.logs.length} lines
          </span>
          {group.durationMs != null ? (
            <span className="shrink-0 font-mono text-[10px] text-muted-foreground">
              {formatDuration(group.durationMs / 1000)}
            </span>
          ) : null}
        </Button>
      ))}
    </div>
  )
}
