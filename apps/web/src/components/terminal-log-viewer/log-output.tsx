import { AnsiLine } from './ansi-line'
import { isErrorLine } from './log-model'
import { HugeiconsIcon } from '@hugeicons/react'
import { ArrowDownToLineIcon } from '@hugeicons/core-free-icons'
import type { RefObject } from 'react'
import type { Virtualizer } from '@tanstack/react-virtual'

import type { BuildLogChunk, BuildStatus } from '@oore/client/models'
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
import { Row } from '@/components/layout/row'
import { textVariants } from '@/lib/recipes'

interface LogOutputProps {
  logs: Array<BuildLogChunk>
  selectedStep: string
  selectedStepMeta: SelectedStepMeta | null
  searchQuery: string
  isLoading: boolean
  logsUnavailable: boolean
  isTerminal: boolean
  buildStatus?: BuildStatus
  wrapLines: boolean
  lineNumbers: ReadonlyMap<BuildLogChunk, number>
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
  buildStatus,
  wrapLines,
  lineNumbers,
  matchingIndexes,
  currentMatchIndex,
  showJumpToLatest,
  onJumpToLatest,
  scrollContainerRef,
  virtualizer,
}: LogOutputProps) {
  'use no memo' // Read fresh row positions from the mutable virtualizer.

  const verticalPadding = 8
  const maxLineNumber =
    logs.length > 0 ? (lineNumbers.get(logs.at(-1)!) ?? 0) : 0
  const lineNumWidth = Math.max(String(maxLineNumber).length, 3)

  return (
    <div className="relative flex h-full min-h-0 flex-col overflow-hidden bg-background">
      {selectedStepMeta ? (
        <Row gap={2} className="shrink-0 border-b bg-muted/20 px-3 py-2">
          <span
            className={cn(textVariants({ variant: 'caption' }), 'font-medium')}
          >
            {selectedStep}
          </span>
          {selectedStepMeta.command ? (
            <code className="min-w-0 truncate font-mono text-[11px] text-foreground">
              $ {selectedStepMeta.command}
            </code>
          ) : null}
        </Row>
      ) : null}

      <ScrollArea
        className="h-full min-h-0 flex-1"
        horizontal={!wrapLines}
        viewportRef={scrollContainerRef}
        role="region"
        aria-label="Build log output"
        data-build-log-output=""
      >
        {logs.length === 0 ? (
          <Empty className="h-full min-h-48 rounded-none border-0 p-6">
            <EmptyHeader>
              <EmptyTitle>
                {
                  emptyState(
                    isLoading,
                    logsUnavailable,
                    isTerminal,
                    buildStatus,
                    selectedStep,
                  ).title
                }
              </EmptyTitle>
              <EmptyDescription>
                {
                  emptyState(
                    isLoading,
                    logsUnavailable,
                    isTerminal,
                    buildStatus,
                    selectedStep,
                  ).description
                }
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
                    // oxlint-disable-next-line layout/max-class-utilities -- sticky line-number gutter
                    className={cn(
                      'sticky left-0 z-10 shrink-0 border-r px-3 text-right text-muted-foreground select-none',
                      isCurrentMatch ? 'bg-primary/5' : 'bg-background',
                      isError && 'text-destructive',
                    )}
                    style={{ width: `${lineNumWidth + 4}ch` }}
                  >
                    {lineNumbers.get(chunk) ?? virtualRow.index + 1}
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
  buildStatus?: BuildStatus,
  selectedStep = 'all',
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
  if (selectedStep !== 'all') {
    return isTerminal
      ? {
          title: 'No output from this step',
          description: 'This step finished without printing any lines.',
        }
      : {
          title: 'Waiting for output',
          description: 'This step has not printed anything yet.',
        }
  }
  if (buildStatus === 'expired') {
    return {
      title: 'No logs',
      description: 'This build expired before a runner started it.',
    }
  }
  if (buildStatus === 'canceled') {
    return {
      title: 'No log output',
      description: 'The build was canceled before it printed any lines.',
    }
  }
  if (isTerminal) {
    return {
      title: 'No log output',
      description: 'This build finished without recording any log lines.',
    }
  }
  if (buildStatus && buildStatus !== 'running') {
    return {
      title: 'Not started yet',
      description: 'Logs appear here once a runner starts the build.',
    }
  }
  return {
    title: 'Waiting for output',
    description: 'New log lines will appear here as the build runs.',
  }
}
