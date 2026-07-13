import { AnsiLine } from './ansi-line'
import { isErrorLine } from './log-model'
import type { RefObject } from 'react'
import type { Virtualizer } from '@tanstack/react-virtual'

import type { BuildLogChunk } from '@/lib/types'
import type { SelectedStepMeta } from './types'
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
  scrollContainerRef,
  virtualizer,
}: LogOutputProps) {
  const maxSeq = logs.length > 0 ? logs[logs.length - 1].sequence : 0
  const lineNumWidth = Math.max(String(maxSeq).length, 3)

  return (
    <div className="flex h-full min-h-0 flex-col overflow-hidden bg-card">
      {selectedStepMeta ? (
        <div className="flex shrink-0 items-center gap-2 border-b bg-muted/20 px-4 py-2">
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

      {logs.length === 0 ? (
        <div className="flex h-48 items-center justify-center">
          <span className="text-xs text-muted-foreground">
            {searchQuery
              ? 'No matching lines'
              : isLoading
                ? 'Loading build logs...'
                : logsUnavailable
                  ? 'Logs are unavailable for this build.'
                  : isTerminal
                    ? 'This build completed without recorded logs.'
                    : 'No logs yet'}
          </span>
        </div>
      ) : (
        <ScrollArea
          className="h-full min-h-0 flex-1"
          horizontal={!wrapLines}
          viewportRef={scrollContainerRef}
          role="region"
          aria-label="Build log output"
        >
          <div
            style={{
              height: `${virtualizer.getTotalSize()}px`,
              width: '100%',
              position: 'relative',
            }}
          >
            {virtualizer.getVirtualItems().map((virtualRow) => {
              const chunk = logs[virtualRow.index]
              const isError = isErrorLine(chunk.content)
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
                    transform: `translateY(${virtualRow.start}px)`,
                  }}
                  className={cn(
                    'flex font-mono text-[13px] leading-[20px] text-card-foreground',
                    isError && 'bg-destructive/10 text-destructive',
                  )}
                >
                  <span
                    className="shrink-0 select-none px-3 text-right text-muted-foreground"
                    style={{ width: `${lineNumWidth + 4}ch` }}
                  >
                    {chunk.sequence}
                  </span>
                  <span
                    className={
                      wrapLines
                        ? 'whitespace-pre-wrap break-all pr-4'
                        : 'whitespace-pre pr-4'
                    }
                  >
                    <AnsiLine content={chunk.content} />
                  </span>
                </div>
              )
            })}
          </div>
        </ScrollArea>
      )}
    </div>
  )
}
