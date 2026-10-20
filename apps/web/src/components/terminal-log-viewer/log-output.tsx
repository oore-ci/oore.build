import { AnsiLine } from './ansi-line'
import { isErrorLine } from './log-model'
import { HugeiconsIcon } from '@hugeicons/react'
import { ArrowDownToLineIcon } from '@hugeicons/core-free-icons'
import type { RefObject } from 'react'
import type { Virtualizer } from '@tanstack/react-virtual'

import type { BuildLogChunk } from '@oore/client/models'
import type { SelectedStepMeta } from './types'
import { Button } from '@/components/ui/button'
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyTitle,
} from '@/components/ui/empty'
import { ScrollArea } from '@/components/ui/scroll-area'
import { cn } from '@/lib/utils'

interface LogOutputProps {
  logs: Array<BuildLogChunk>
  selectedStep: string
  selectedStepMeta: SelectedStepMeta | null
  searchQuery: string
  isLoading: boolean
  logsUnavailable: boolean
  isTerminal: boolean
  wrapLines: boolean
  matchingIndexes: ReadonlySet<number>
  currentMatchIndex: number | null
  showJumpToLatest: boolean
  onJumpToLatest: () => void
  scrollContainerRef: RefObject<HTMLDivElement | null>
  virtualizer: Virtualizer<HTMLDivElement, Element>
}

export function LogOutput({
  logs,
  selectedStep,
  selectedStepMeta,
  searchQuery,
  isLoading,
  logsUnavailable,
  isTerminal,
  wrapLines,
  matchingIndexes,
  currentMatchIndex,
  showJumpToLatest,
  onJumpToLatest,
  scrollContainerRef,
  virtualizer,
}: LogOutputProps) {
  'use no memo' // Read fresh row positions from the mutable virtualizer.

  const verticalPadding = 8
  const maxSeq = logs.length > 0 ? logs[logs.length - 1].sequence : 0
  const lineNumWidth = Math.max(String(maxSeq).length, 3)

  return (
    <div className="relative flex h-full min-h-0 flex-col overflow-hidden bg-background">
      {selectedStepMeta ? (
        <div className="flex shrink-0 items-center gap-2 border-b bg-muted/20 px-3 py-2">
          <span className="text-xs font-medium text-muted-foreground">
            {selectedStep}
          </span>
          {selectedStepMeta.command ? (
            <code className="min-w-0 truncate font-mono text-[11px] text-foreground">
              $ {selectedStepMeta.command}
            </code>
          ) : null}
        </div>
      ) : null}

      <ScrollArea
        className="h-full min-h-0 flex-1"
        horizontal={!wrapLines}
        viewportRef={scrollContainerRef}
        role="region"
        aria-label="Build log output"
      >
        {logs.length === 0 ? (
          <Empty className="h-full min-h-48 rounded-none border-0 p-6">
            <EmptyHeader>
              <EmptyTitle>
                {emptyState(isLoading, logsUnavailable, isTerminal).title}
              </EmptyTitle>
              <EmptyDescription>
                {emptyState(isLoading, logsUnavailable, isTerminal).description}
              </EmptyDescription>
            </EmptyHeader>
          </Empty>
        ) : (
          <div
            style={{
              height: `${virtualizer.getTotalSize() + verticalPadding * 2}px`,
              width: '100%',
              position: 'relative',
            }}
          >
            {virtualizer.getVirtualItems().map((virtualRow) => {
              const chunk = logs[virtualRow.index]
              const isError = isErrorLine(chunk.content)
              const isMatch = matchingIndexes.has(virtualRow.index)
              const isCurrentMatch = currentMatchIndex === virtualRow.index
              return (
                <div
                  key={virtualRow.key}
                  data-index={virtualRow.index}
                  ref={virtualizer.measureElement}
                  style={{
                    position: 'absolute',
                    top: 0,
                    left: 0,
                    width: '100%',
                    transform: `translateY(${virtualRow.start + verticalPadding}px)`,
                  }}
                  className={cn(
                    'flex border-l-2 border-l-transparent font-mono text-[13px] leading-5 text-foreground',
                    isError && 'border-l-destructive',
                    isCurrentMatch && 'bg-primary/5',
                  )}
                  aria-current={isCurrentMatch ? 'true' : undefined}
                >
                  <span
                    className={cn(
                      'sticky left-0 z-10 shrink-0 border-r px-3 text-right text-muted-foreground select-none',
                      isCurrentMatch ? 'bg-primary/5' : 'bg-background',
                      isError && 'text-destructive',
                    )}
                    style={{ width: `${lineNumWidth + 4}ch` }}
                  >
                    {chunk.sequence}
                  </span>
                  <span
                    className={cn(
                      'min-w-0 pr-4 pl-3',
                      wrapLines
                        ? 'break-all whitespace-pre-wrap'
                        : 'whitespace-pre',
                    )}
                  >
                    <AnsiLine
                      content={chunk.content}
                      searchQuery={isMatch ? searchQuery : ''}
                    />
                  </span>
                </div>
              )
            })}
          </div>
        )}
      </ScrollArea>

      {showJumpToLatest && logs.length > 0 ? (
        <Button
          variant="secondary"
          size="sm"
          className="absolute right-3 bottom-3 shadow-sm"
          onClick={onJumpToLatest}
        >
          <HugeiconsIcon icon={ArrowDownToLineIcon} data-icon="inline-start" />
          Jump to latest
        </Button>
      ) : null}
    </div>
  )
}

function emptyState(
  isLoading: boolean,
  logsUnavailable: boolean,
  isTerminal: boolean,
) {
  if (isLoading) {
    return {
      title: 'Loading build logs',
      description: 'The first log lines will appear here shortly.',
    }
  }
  if (logsUnavailable) {
    return {
      title: 'Logs unavailable',
      description: 'Oore could not retrieve the logs for this build.',
    }
  }
  if (isTerminal) {
    return {
      title: 'No log output',
      description: 'This build completed without recording any log lines.',
    }
  }
  return {
    title: 'Waiting for output',
    description: 'New log lines will appear here as the build runs.',
  }
}
