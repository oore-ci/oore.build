import {
  lazy,
  Suspense,
  useDeferredValue,
  useEffect,
  useMemo,
  useRef,
  useState,
} from 'react'
import { useVirtualizer } from '@tanstack/react-virtual'

import { LogOutput } from './log-output'
import { LogToolbar } from './log-toolbar'
import { defaultSelectedStep, groupLogs } from './log-model'
import { StepNavigation } from './step-navigation'
import type { SelectedStepMeta, TerminalLogViewerProps } from './types'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { Skeleton } from '@/components/ui/skeleton'
import { useAutoScroll } from '@/hooks/use-auto-scroll'
import { useIsMobile } from '@/hooks/use-mobile'
import { toast } from '@/lib/toast'
import { cn } from '@/lib/utils'

const StepSelect = lazy(() =>
  import('./step-select').then((module) => ({
    default: module.StepSelect,
  })),
)

export default function TerminalLogViewer({
  logs,
  stepResults,
  isStreaming,
  fillAvailableHeight = false,
  isLoading = false,
  logsUnavailable = false,
  onRetryLogs,
  isTerminal = false,
}: TerminalLogViewerProps) {
  const [userSelectedStep, setUserSelectedStep] = useState<string | null>(null)
  const [autoScroll, setAutoScroll] = useState(true)
  const [wrapLines, setWrapLines] = useState(false)
  const [searchQuery, setSearchQuery] = useState('')
  const [activeMatchOrdinal, setActiveMatchOrdinal] = useState(0)
  const deferredSearchQuery = useDeferredValue(searchQuery)
  const scrollContainerRef = useRef<HTMLDivElement>(null)
  const searchInputRef = useRef<HTMLInputElement>(null)
  const isMobile = useIsMobile()

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
  const matchingIndexes = useMemo(() => {
    const query = deferredSearchQuery.trim().toLocaleLowerCase()
    if (!query) return []
    const indexes: Array<number> = []
    selectedLogs.forEach((chunk, index) => {
      if (chunk.content.toLocaleLowerCase().includes(query)) indexes.push(index)
    })
    return indexes
  }, [selectedLogs, deferredSearchQuery])
  const matchingIndexSet = useMemo(
    () => new Set(matchingIndexes),
    [matchingIndexes],
  )
  const resolvedMatchOrdinal =
    matchingIndexes.length === 0
      ? 0
      : Math.min(activeMatchOrdinal, matchingIndexes.length - 1)
  const currentMatchIndex =
    matchingIndexes.length > 0 ? matchingIndexes[resolvedMatchOrdinal] : null
  const selectedStepMeta: SelectedStepMeta | null = useMemo(() => {
    if (selectedStep === 'all') return null
    const group = stepGroupsByName.get(selectedStep)
    if (!group) return null
    return {
      command: group.command,
    }
  }, [selectedStep, stepGroupsByName])

  // oxlint-disable-next-line react/incompatible-library
  const virtualizer = useVirtualizer({
    count: selectedLogs.length,
    getScrollElement: () => scrollContainerRef.current,
    estimateSize: () => 20,
    overscan: 50,
  })
  useAutoScroll(virtualizer, selectedLogs.length, autoScroll)
  function handleScroll() {
    const element = scrollContainerRef.current
    if (!element) return
    setAutoScroll(
      element.scrollHeight - element.scrollTop - element.clientHeight < 40,
    )
  }

  useEffect(() => {
    if (currentMatchIndex === null) return
    setAutoScroll(false)
    virtualizer.scrollToIndex(currentMatchIndex, { align: 'center' })
  }, [currentMatchIndex, virtualizer])

  const logStepGroups = stepGroups.filter((group) => group.logs.length > 0)
  const hasSteps = logStepGroups.length > 0
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

  function copyLogs() {
    const rawLogs = selectedLogs.map((chunk) => chunk.content).join('\n')
    void navigator.clipboard.writeText(rawLogs).then(
      () => toast.success('Build logs copied'),
      () => toast.error('Could not copy build logs'),
    )
  }

  const lineCountLabel = `${selectedLogs.length} ${
    selectedLogs.length === 1 ? 'line' : 'lines'
  }`

  function handleSearchQueryChange(query: string) {
    setSearchQuery(query)
    setActiveMatchOrdinal(0)
  }

  function handleSearchClear() {
    setSearchQuery('')
    setActiveMatchOrdinal(0)
    searchInputRef.current?.focus()
  }

  function navigateMatch(direction: -1 | 1) {
    if (matchingIndexes.length === 0) return
    setActiveMatchOrdinal(
      (resolvedMatchOrdinal + direction + matchingIndexes.length) %
        matchingIndexes.length,
    )
  }

  function handleSelectStep(step: string) {
    setUserSelectedStep(step)
    setActiveMatchOrdinal(0)
    setAutoScroll(true)
  }

  function jumpToLatest() {
    setAutoScroll(true)
    if (selectedLogs.length > 0) {
      virtualizer.scrollToIndex(selectedLogs.length - 1, { align: 'end' })
    }
  }

  function toggleFollow() {
    if (autoScroll) {
      setAutoScroll(false)
      return
    }
    jumpToLatest()
  }

  return (
    <section
      aria-labelledby="build-logs-heading"
      className={cn(
        'flex flex-col overflow-hidden rounded-xl border bg-card shadow-xs',
        fillAvailableHeight
          ? 'h-full min-h-80'
          : 'h-[clamp(28rem,62dvh,50rem)]',
      )}
    >
      <div className="flex shrink-0 flex-col gap-2 border-b bg-muted/20 px-3 py-2.5 sm:flex-row sm:items-center">
        <div className="flex shrink-0 items-center gap-2">
          <h2 id="build-logs-heading" className="text-sm font-medium">
            Build logs
          </h2>
          {isStreaming ? (
            <Badge variant="outline">
              <span className="relative flex size-1.5">
                <span className="absolute inline-flex size-full rounded-full bg-success opacity-75 motion-safe:animate-ping" />
                <span className="relative inline-flex size-1.5 rounded-full bg-success" />
              </span>
              Live
            </Badge>
          ) : null}
          <span className="text-xs text-muted-foreground tabular-nums">
            {lineCountLabel}
          </span>
        </div>
        <div className="min-w-0 flex-1 sm:ml-auto">
          <LogToolbar
            searchQuery={searchQuery}
            searchInputRef={searchInputRef}
            matchCount={matchingIndexes.length}
            activeMatchPosition={
              matchingIndexes.length > 0 ? resolvedMatchOrdinal + 1 : 0
            }
            wrapLines={wrapLines}
            followLive={autoScroll}
            isStreaming={isStreaming}
            onSearchQueryChange={handleSearchQueryChange}
            onSearchClear={handleSearchClear}
            onPreviousMatch={() => navigateMatch(-1)}
            onNextMatch={() => navigateMatch(1)}
            onToggleWrap={() => setWrapLines((value) => !value)}
            onToggleFollow={toggleFollow}
            onCopy={copyLogs}
            onDownload={downloadRawLogs}
          />
        </div>
      </div>

      {logsUnavailable ? (
        <div
          className="flex shrink-0 items-center justify-between gap-2 border-b px-3 py-2"
          role="status"
        >
          <p className="text-sm">Logs could not be loaded.</p>
          {onRetryLogs ? (
            <Button variant="outline" size="sm" onClick={onRetryLogs}>
              Retry logs
            </Button>
          ) : null}
        </div>
      ) : !isTerminal && !isStreaming ? (
        <p
          className="shrink-0 border-b px-3 py-2 text-sm text-muted-foreground"
          role="status"
        >
          Live log connection unavailable. Checking for updates periodically.
        </p>
      ) : null}

      <div className="flex min-h-0 flex-1 flex-col md:flex-row">
        {hasSteps ? (
          isMobile ? (
            <Suspense
              fallback={
                <div className="shrink-0 border-b bg-muted/20 p-2">
                  <Skeleton className="h-8 w-full" />
                </div>
              }
            >
              <StepSelect
                groups={logStepGroups}
                selectedStep={selectedStep}
                allLogCount={allVisibleLogs.length}
                onSelect={handleSelectStep}
              />
            </Suspense>
          ) : (
            <StepNavigation
              groups={logStepGroups}
              selectedStep={selectedStep}
              allLogCount={allVisibleLogs.length}
              onSelect={handleSelectStep}
            />
          )
        ) : null}

        <div
          className="min-h-0 min-w-0 flex-1 overflow-hidden"
          onScrollCapture={handleScroll}
        >
          <LogOutput
            logs={selectedLogs}
            selectedStep={selectedStep}
            selectedStepMeta={selectedStepMeta}
            searchQuery={deferredSearchQuery}
            isLoading={isLoading}
            logsUnavailable={logsUnavailable}
            isTerminal={isTerminal}
            wrapLines={wrapLines}
            matchingIndexes={matchingIndexSet}
            currentMatchIndex={currentMatchIndex}
            showJumpToLatest={!autoScroll}
            onJumpToLatest={jumpToLatest}
            scrollContainerRef={scrollContainerRef}
            virtualizer={virtualizer}
          />
        </div>
      </div>
    </section>
  )
}
