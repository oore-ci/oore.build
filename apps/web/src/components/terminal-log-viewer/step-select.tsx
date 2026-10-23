import { HugeiconsIcon } from '@hugeicons/react'
import { FileTerminalIcon } from '@hugeicons/core-free-icons'

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
import { textVariants } from '@/lib/recipes'
import { cn } from '@/lib/utils'

interface StepSelectProps {
  groups: Array<StepGroup>
  selectedStep: string
  allLogCount: number
  onSelect: (step: string) => void
}

export function StepSelect({
  groups,
  selectedStep,
  allLogCount,
  onSelect,
}: StepSelectProps) {
  const items = Object.fromEntries([
    ['all', `Full log · ${formatLineCount(allLogCount)}`],
    ...groups.map(
      (group) =>
        [
          group.name,
          `${group.name} · ${formatLineCount(group.logs.length)}`,
        ] as const,
    ),
  ])

  return (
    <div className="shrink-0 border-b bg-muted/20 p-2">
      <Select
        value={selectedStep}
        onValueChange={(value) => onSelect(value ?? 'all')}
        items={items}
      >
        <SelectTrigger className="w-full bg-background" aria-label="Build step">
          <SelectValue />
        </SelectTrigger>
        <SelectContent align="start">
          <SelectGroup>
            <SelectItem value="all">
              <HugeiconsIcon icon={FileTerminalIcon} />
              Full log
              <span
                className={cn(
                  textVariants({ variant: 'caption' }),
                  'ml-auto tabular-nums',
                )}
              >
                {allLogCount}
              </span>
            </SelectItem>
            {groups.map((group) => (
              <SelectItem key={group.name} value={group.name}>
                <StepStatusIcon status={group.status} />
                <span className="min-w-0 flex-1 truncate">{group.name}</span>
                <span
                  className={cn(
                    textVariants({ variant: 'caption' }),
                    'ml-auto tabular-nums',
                  )}
                >
                  {group.logs.length}
                </span>
              </SelectItem>
            ))}
          </SelectGroup>
        </SelectContent>
      </Select>
    </div>
  )
}

function formatLineCount(count: number) {
  return `${count} ${count === 1 ? 'line' : 'lines'}`
}
