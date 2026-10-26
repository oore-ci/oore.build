import { HugeiconsIcon } from '@hugeicons/react'
import { FileTerminalIcon } from '@hugeicons/core-free-icons'

import { StepStatusIcon } from './step-status-icon'
import { StepDurationBar, StepMedianDelta } from './step-timing'
import type { StepGroup } from './types'
import { Stack } from '@/components/layout/stack'
import { Text } from '@/components/layout/text'
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { formatDuration } from '@/lib/format-utils'
import { longestStepMs, stepTimingSummary } from '@/lib/step-timings'

interface StepSelectProps {
  groups: Array<StepGroup>
  medians?: ReadonlyMap<string, number>
  selectedStep: string
  allLogCount: number
  onSelect: (step: string) => void
}

export function StepSelect({
  groups,
  medians,
  selectedStep,
  allLogCount,
  onSelect,
}: StepSelectProps) {
  // Same scale as the desktop step list: the slowest step or median fills it.
  const longestMs = longestStepMs(groups, medians)
  const items = Object.fromEntries([
    [
      'all',
      `Full log · ${stepTimingSummary(undefined, undefined, allLogCount)}`,
    ],
    ...groups.map(
      (group) =>
        [
          group.name,
          `${group.name} · ${stepTimingSummary(
            group.durationMs,
            medians?.get(group.name),
            group.logs.length,
          )}`,
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
          <SelectValue className="min-w-0 truncate" />
        </SelectTrigger>
        <SelectContent align="start">
          <SelectGroup>
            <SelectItem value="all">
              <HugeiconsIcon icon={FileTerminalIcon} />
              Full log
              <Text variant="mono" render={<span />} className="ml-auto">
                {formatLineCount(allLogCount)}
              </Text>
            </SelectItem>
            {groups.map((group) => (
              <StepOption
                key={group.name}
                group={group}
                longestMs={longestMs}
                medianMs={medians?.get(group.name)}
              />
            ))}
          </SelectGroup>
        </SelectContent>
      </Select>
    </div>
  )
}

function StepOption({
  group,
  longestMs,
  medianMs,
}: {
  group: StepGroup
  longestMs: number
  medianMs?: number
}) {
  const { durationMs } = group
  return (
    <SelectItem value={group.name} className="items-start">
      <StepStatusIcon status={group.status} />
      <Stack className="min-w-0 flex-1">
        <span className="truncate">{group.name}</span>
        <Text variant="mono" render={<span />} className="text-[10px]">
          {formatLineCount(group.logs.length)}
        </Text>
        {durationMs != null && longestMs > 0 ? (
          <StepDurationBar
            durationMs={durationMs}
            longestMs={longestMs}
            medianMs={medianMs}
          />
        ) : null}
      </Stack>
      {durationMs != null ? (
        <Stack
          gap={0.5}
          className="items-end font-mono text-[10px] text-muted-foreground"
        >
          <span>{formatDuration(durationMs / 1000)}</span>
          {medianMs != null ? (
            <StepMedianDelta durationMs={durationMs} medianMs={medianMs} />
          ) : null}
        </Stack>
      ) : null}
    </SelectItem>
  )
}

function formatLineCount(count: number) {
  return `${count} ${count === 1 ? 'line' : 'lines'}`
}
