import { formatDuration } from './log-model'
import { StepStatusIcon } from './step-status-icon'
import type { StepGroup } from './types'
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Button } from '@/components/ui/button'

interface StepNavigationProps {
  groups: Array<StepGroup>
  selectedStep: string
  allLogCount: number
  onSelect: (step: string) => void
}

export function MobileStepSelector(props: StepNavigationProps) {
  return (
    <div className="border border-b-0 bg-muted/30 px-3 py-2 md:hidden">
      <Select
        aria-label="Build step"
        value={props.selectedStep}
        onValueChange={(value) => props.onSelect(value ?? 'all')}
      >
        <SelectTrigger className="h-11 w-full" aria-label="Build step">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectGroup>
            <SelectItem value="all">All logs ({props.allLogCount})</SelectItem>
            {props.groups.map((group) => (
              <SelectItem key={group.name} value={group.name}>
                <StepStatusIcon status={group.status} />
                {group.name}
                {group.durationMs != null
                  ? ` (${formatDuration(group.durationMs / 1000)})`
                  : ''}
              </SelectItem>
            ))}
          </SelectGroup>
        </SelectContent>
      </Select>
    </div>
  )
}

export function StepSidebar(props: StepNavigationProps) {
  return (
    <aside className="hidden flex-col overflow-y-auto border-r bg-muted/20 p-1 md:flex">
      <Button
        variant={props.selectedStep === 'all' ? 'secondary' : 'ghost'}
        size="xs"
        onClick={() => props.onSelect('all')}
        className="h-auto w-full justify-start rounded-none px-2 py-1.5"
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
          className="h-auto w-full justify-start rounded-none px-2 py-1.5"
          title={group.command}
        >
          <StepStatusIcon status={group.status} />
          <span className="min-w-0 flex-1 truncate">{group.name}</span>
          {group.durationMs != null ? (
            <span className="shrink-0 font-mono text-[10px] text-muted-foreground">
              {formatDuration(group.durationMs / 1000)}
            </span>
          ) : null}
        </Button>
      ))}
    </aside>
  )
}
