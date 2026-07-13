import { useCallback, useMemo, useRef, useState } from 'react'
import { useVirtualizer } from '@tanstack/react-virtual'

import { LogOutput } from './log-output'
import { LogToolbar } from './log-toolbar'
import {
  defaultSelectedStep,
  findFirstErrorIndex,
  groupLogs,
  isErrorLine,
} from './log-model'
import { StepNavigation } from './step-navigation'
import type { SelectedStepMeta, TerminalLogViewerProps } from './types'
import { useWindowEvent } from '@/hooks/use-window-event'
import { useMountEffect } from '@/hooks/use-mount-effect'
import { useAutoScroll } from '@/hooks/use-auto-scroll'

export default function TerminalLogViewer({
  logs,
  stepResults,
  isStreaming,
  isLoading = false,
  logsUnavailable = false,
  isTerminal = false,
}: TerminalLogViewerProps) {
  const [userSelectedStep, setUserSelectedStep] = useState<string | null>(null)
  const [autoScroll, setAutoScroll] = useState(true)
  const [wrapLines, setWrapLines] = useState(false)
  const [searchQuery, setSearchQuery] = useState('')
  const scrollContainerRef = useRef<HTMLDivElement>(null)
  const searchInputRef = useRef<HTMLInputElement>(null)

  const { stepGroups, stepGroupsByName, allVisibleLogs, runningStepName } =
    useMemo(() => groupLogs(logs, stepResults), [logs, stepResults])

  const selectedStep = useMemo(() => {
    if (
      userSelectedStep === 'all' ||
      (userSelectedStep !== null && stepGroupsByName.has(userSelectedStep))
    ) {
      return userSelectedStep
    }
    return defaultSelectedStep(stepGroups, runningStepName)
  }, [userSelectedStep, stepGroups, stepGroupsByName, runningStepName])

  const selectedLogs = useMemo(
    () =>
      selectedStep === 'all'
        ? allVisibleLogs
        : (stepGroupsByName.get(selectedStep)?.logs ?? []),
    [selectedStep, allVisibleLogs, stepGroupsByName],
  )
  const filteredLogs = useMemo(() => {
    if (!searchQuery.trim()) return selectedLogs
    const query = searchQuery.toLowerCase()
    return selectedLogs.filter((chunk) =>
      chunk.content.toLowerCase().includes(query),
    )
  }, [selectedLogs, searchQuery])
  const selectedStepMeta: SelectedStepMeta | null = useMemo(() => {
    if (selectedStep === 'all') return null
    const group = stepGroupsByName.get(selectedStep)
    if (!group) return null
    return {
      command: group.command,
    }
  }, [selectedStep, stepGroupsByName])

  const virtualizer = useVirtualizer({
    count: filteredLogs.length,
    getScrollElement: () => scrollContainerRef.current,
    estimateSize: () => 20,
    overscan: 50,
  })
  useAutoScroll(virtualizer, filteredLogs.length, autoScroll)

  const handleScroll = useCallback(() => {
    const element = scrollContainerRef.current
    if (!element) return
    setAutoScroll(
      element.scrollHeight - element.scrollTop - element.clientHeight < 40,
    )
  }, [])

  useMountEffect(() => {
    const element = scrollContainerRef.current
    if (!element) return
    element.addEventListener('scroll', handleScroll)
    return () => element.removeEventListener('scroll', handleScroll)
  })

  useWindowEvent('keydown', (event) => {
    if ((event.ctrlKey || event.metaKey) && event.key === 'f') {
      const target = event.target as HTMLElement | null
      if (target?.closest('input, textarea, [contenteditable="true"]')) return
      const element = scrollContainerRef.current
      if (!element) return
      const rect = element.getBoundingClientRect()
      if (rect.top < window.innerHeight && rect.bottom > 0) {
        event.preventDefault()
        searchInputRef.current?.focus()
      }
    }
    if (event.key === 'Escape' && searchQuery) {
      setSearchQuery('')
      searchInputRef.current?.focus()
    }
  })

  const logStepGroups = stepGroups.filter((group) => group.logs.length > 0)
  const hasSteps = logStepGroups.length > 0
  const hasErrors = filteredLogs.some((chunk) => isErrorLine(chunk.content))

  function jumpToFirstError() {
    const index = findFirstErrorIndex(filteredLogs)
    if (index < 0) return
    setAutoScroll(false)
    virtualizer.scrollToIndex(index, { align: 'center' })
  }

  function downloadRawLogs() {
    const blob = new Blob(
      [selectedLogs.map((chunk) => chunk.content).join('\n')],
      {
        type: 'text/plain',
      },
    )
    const url = URL.createObjectURL(blob)
    const anchor = document.createElement('a')
    anchor.href = url
    anchor.download = 'build-logs.txt'
    anchor.click()
    URL.revokeObjectURL(url)
  }

  function scrollToLatest() {
    setAutoScroll(true)
    virtualizer.scrollToIndex(filteredLogs.length - 1, { align: 'end' })
  }

  return (
    <section
      aria-labelledby="build-logs-heading"
      className="flex h-[68dvh] min-h-[32rem] max-h-[56rem] flex-col overflow-hidden border bg-card"
    >
      <div className="flex shrink-0 flex-col gap-3 border-b bg-muted/20 px-4 py-3 lg:flex-row lg:items-center">
        <div className="min-w-0">
          <h2 id="build-logs-heading" className="text-sm font-medium">
            Build logs
          </h2>
          <p className="text-xs text-muted-foreground">
            {isTerminal
              ? 'Complete output with step-level context.'
              : 'Live output with step-level context.'}
          </p>
        </div>
        <div className="min-w-0 flex-1">
          <LogToolbar
            isStreaming={isStreaming}
            logCount={filteredLogs.length}
            totalLogCount={selectedLogs.length}
            searchQuery={searchQuery}
            searchInputRef={searchInputRef}
            wrapLines={wrapLines}
            showScrollLatest={!autoScroll && filteredLogs.length > 0}
            hasErrors={hasErrors}
            onSearchQueryChange={setSearchQuery}
            onSearchClear={() => setSearchQuery('')}
            onJumpToError={jumpToFirstError}
            onToggleWrap={() => setWrapLines((value) => !value)}
            onDownload={downloadRawLogs}
            onScrollLatest={scrollToLatest}
          />
        </div>
      </div>

      {hasSteps ? (
        <StepNavigation
          groups={logStepGroups}
          selectedStep={selectedStep}
          allLogCount={allVisibleLogs.length}
          onSelect={setUserSelectedStep}
        />
      ) : null}

      <div className="min-h-0 flex-1 overflow-hidden">
        <LogOutput
          logs={filteredLogs}
          selectedStep={selectedStep}
          selectedStepMeta={selectedStepMeta}
          searchQuery={searchQuery}
          isLoading={isLoading}
          logsUnavailable={logsUnavailable}
          isTerminal={isTerminal}
          wrapLines={wrapLines}
          scrollContainerRef={scrollContainerRef}
          virtualizer={virtualizer}
        />
      </div>
    </section>
  )
}
