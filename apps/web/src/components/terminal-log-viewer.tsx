import { useCallback, useMemo, useRef, useState } from 'react'
import { useVirtualizer } from '@tanstack/react-virtual'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  AlertCircleIcon,
  ArrowDown01Icon,
  Cancel01Icon,
  CheckmarkCircle02Icon,
  Download04Icon,
  Loading03Icon,
  Search01Icon,
  TextWrapIcon,
} from '@hugeicons/core-free-icons'

import type { BuildLogChunk, StepResult } from '@/lib/types'
import { useMountEffect } from '@/hooks/use-mount-effect'
import { useAutoScroll } from '@/hooks/use-auto-scroll'
import { parseAnsi } from '@/lib/ansi-to-html'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { ScrollArea } from '@/components/ui/scroll-area'
import { Alert, AlertDescription } from '@/components/ui/alert'
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'

// ── Types ──────────────────────────────────────────────────────

export interface TerminalLogViewerProps {
  logs: Array<BuildLogChunk>
  stepResults: Array<StepResult>
  isStreaming: boolean
  streamError?: string
  logsUnavailable?: boolean
  isTerminal?: boolean
}

interface StepGroup {
  name: string
  status: string
  command?: string
  durationMs?: number
  logs: Array<BuildLogChunk>
}

// ── Helpers ────────────────────────────────────────────────────

function parseStepMarker(content: string): {
  event: 'start' | 'end'
  name: string
  status?: string
  command?: string
} | null {
  const prefix = '[oore-step] '
  if (!content.startsWith(prefix)) return null
  try {
    const raw = content.slice(prefix.length)
    const parsed = JSON.parse(raw) as {
      event?: string
      name?: string
      status?: string
      command?: string
    }
    if (
      (parsed.event === 'start' || parsed.event === 'end') &&
      parsed.name?.trim()
    ) {
      return {
        event: parsed.event,
        name: parsed.name.trim(),
        status: parsed.status?.trim(),
        command: parsed.command?.trim(),
      }
    }
    return null
  } catch {
    return null
  }
}

function formatDuration(seconds: number): string {
  if (seconds < 60) return `${Math.round(seconds)}s`
  const mins = Math.floor(seconds / 60)
  const secs = Math.round(seconds % 60)
  if (mins < 60) return `${mins}m ${secs}s`
  const hrs = Math.floor(mins / 60)
  const remainMins = mins % 60
  return `${hrs}h ${remainMins}m`
}

function stepStatusVariant(status: string) {
  const normalized = status.trim().toLowerCase()
  if (normalized === 'running') return 'info'
  if (normalized === 'succeeded') return 'success'
  if (
    normalized === 'failed' ||
    normalized === 'canceled' ||
    normalized === 'timed_out'
  ) {
    return 'destructive'
  }
  return 'outline'
}

const ERROR_PATTERN =
  /\b(error|ERROR|FAILED|FAILURE|fatal|FATAL|exception|EXCEPTION|panic|PANIC)\b/

function findFirstErrorIndex(lines: Array<BuildLogChunk>): number {
  return lines.findIndex(
    (chunk) => chunk.stream === 'stderr' || ERROR_PATTERN.test(chunk.content),
  )
}

// ── ANSI line renderer ─────────────────────────────────────────

function AnsiLine({ content }: { content: string }) {
  const spans = parseAnsi(content)
  if (spans.length === 1 && !spans[0].fg && !spans[0].bold) {
    return <>{content}</>
  }
  return (
    <>
      {spans.map((span, i) => {
        const style: React.CSSProperties = {}
        if (span.fg) style.color = span.fg
        if (span.bg) style.backgroundColor = span.bg
        if (span.bold) style.fontWeight = 700
        if (span.dim) style.opacity = 0.6
        if (span.italic) style.fontStyle = 'italic'
        if (span.underline) style.textDecoration = 'underline'
        const hasStyle = Object.keys(style).length > 0
        return hasStyle ? (
          <span key={i} style={style}>
            {span.text}
          </span>
        ) : (
          <span key={i}>{span.text}</span>
        )
      })}
    </>
  )
}

// ── Component ──────────────────────────────────────────────────

export default function TerminalLogViewer({
  logs,
  stepResults,
  isStreaming,
  streamError,
  logsUnavailable = false,
  isTerminal = false,
}: TerminalLogViewerProps) {
  const [userSelectedStep, setUserSelectedStep] = useState<string | null>(null)
  const [autoScroll, setAutoScroll] = useState(true)
  const [wrapLines, setWrapLines] = useState(false)
  const [searchQuery, setSearchQuery] = useState('')
  const [searchOpen, setSearchOpen] = useState(false)
  const scrollContainerRef = useRef<HTMLDivElement>(null)
  const searchInputRef = useRef<HTMLInputElement>(null)

  // ── Step grouping ──────────────────────────────────────────

  const { stepGroups, stepGroupsByName, allVisibleLogs, runningStepName } =
    useMemo(() => {
      const groups = new Map<string, StepGroup>()
      const order: Array<string> = []
      const visibleLogs: Array<BuildLogChunk> = []
      let activeStep: string | null = null

      const ensureGroup = (name: string): StepGroup => {
        const existing = groups.get(name)
        if (existing) return existing
        const created: StepGroup = {
          name,
          status: 'pending',
          logs: [],
        }
        groups.set(name, created)
        order.push(name)
        return created
      }

      for (const chunk of logs) {
        const marker = parseStepMarker(chunk.content)
        if (marker) {
          const group = ensureGroup(marker.name)
          if (marker.event === 'start') {
            group.status = 'running'
            if (marker.command) group.command = marker.command
            activeStep = marker.name
          } else {
            group.status = marker.status ?? 'succeeded'
            activeStep = activeStep === marker.name ? null : activeStep
          }
          continue
        }

        visibleLogs.push(chunk)
        if (activeStep) {
          ensureGroup(activeStep).logs.push(chunk)
        }
      }

      for (const result of stepResults) {
        const group = ensureGroup(result.name)
        group.status = result.status
        group.durationMs = result.duration_ms
      }

      const orderedGroups = order
        .map((name) => groups.get(name))
        .filter(Boolean) as Array<StepGroup>

      return {
        stepGroups: orderedGroups,
        stepGroupsByName: groups,
        allVisibleLogs: visibleLogs,
        runningStepName:
          orderedGroups.find((group) => group.status === 'running')?.name ??
          null,
      }
    }, [logs, stepResults])

  // ── Derive selected step ──────────────────────────────────
  const selectedStep = useMemo(() => {
    if (userSelectedStep !== null) {
      const valid =
        userSelectedStep === 'all' || stepGroupsByName.has(userSelectedStep)
      if (valid) return userSelectedStep
    }
    if (runningStepName) return runningStepName
    return (
      stepGroups.find(
        (group) => group.status === 'failed' && group.logs.length > 0,
      )?.name ??
      stepGroups.find((group) => group.logs.length > 0)?.name ??
      'all'
    )
  }, [userSelectedStep, stepGroups, stepGroupsByName, runningStepName])

  // ── Filtered logs ──────────────────────────────────────────

  const selectedLogs = useMemo(() => {
    if (selectedStep === 'all') return allVisibleLogs
    return stepGroupsByName.get(selectedStep)?.logs ?? []
  }, [selectedStep, allVisibleLogs, stepGroupsByName])

  const filteredLogs = useMemo(() => {
    if (!searchQuery.trim()) return selectedLogs
    const q = searchQuery.toLowerCase()
    return selectedLogs.filter((chunk) =>
      chunk.content.toLowerCase().includes(q),
    )
  }, [selectedLogs, searchQuery])

  const selectedStepMeta = useMemo(() => {
    if (selectedStep === 'all') return null
    const group = stepGroupsByName.get(selectedStep)
    if (!group) return null
    return {
      command: group.command,
      status: group.status,
      durationMs: group.durationMs,
    }
  }, [selectedStep, stepGroupsByName])

  // ── Virtualizer ────────────────────────────────────────────

  const virtualizer = useVirtualizer({
    count: filteredLogs.length,
    getScrollElement: () => scrollContainerRef.current,
    estimateSize: () => 20,
    overscan: 50,
  })

  // ── Auto-scroll ────────────────────────────────────────────

  useAutoScroll(virtualizer, filteredLogs.length, autoScroll)

  const handleScroll = useCallback(() => {
    const el = scrollContainerRef.current
    if (!el) return
    const isAtBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 40
    setAutoScroll(isAtBottom)
  }, [])

  // Attach scroll listener to the ScrollArea viewport
  useMountEffect(() => {
    const el = scrollContainerRef.current
    if (!el) return
    el.addEventListener('scroll', handleScroll)
    return () => el.removeEventListener('scroll', handleScroll)
  })

  // ── Keyboard shortcuts ─────────────────────────────────────

  const searchOpenRef = useRef(searchOpen)
  searchOpenRef.current = searchOpen

  useMountEffect(() => {
    function handleKeyDown(e: KeyboardEvent) {
      if ((e.ctrlKey || e.metaKey) && e.key === 'f') {
        const target = e.target as HTMLElement | null
        if (target?.closest('input, textarea, [contenteditable="true"]')) return
        const el = scrollContainerRef.current
        if (!el) return
        // Only intercept when the log viewer is likely in view
        const rect = el.getBoundingClientRect()
        if (rect.top < window.innerHeight && rect.bottom > 0) {
          e.preventDefault()
          setSearchOpen(true)
          setTimeout(() => searchInputRef.current?.focus(), 0)
        }
      }
      if (e.key === 'Escape' && searchOpenRef.current) {
        setSearchOpen(false)
        setSearchQuery('')
      }
    }
    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  })

  // ── Jump to first error ────────────────────────────────────

  function jumpToFirstError() {
    const idx = findFirstErrorIndex(filteredLogs)
    if (idx >= 0) {
      setAutoScroll(false)
      virtualizer.scrollToIndex(idx, { align: 'center' })
    }
  }

  // ── Download raw logs ──────────────────────────────────────

  function downloadRawLogs() {
    const text = selectedLogs.map((c) => c.content).join('\n')
    const blob = new Blob([text], { type: 'text/plain' })
    const url = URL.createObjectURL(blob)
    const a = document.createElement('a')
    a.href = url
    a.download = 'build-logs.txt'
    a.click()
    URL.revokeObjectURL(url)
  }

  // ── Line number width ──────────────────────────────────────

  const maxSeq =
    filteredLogs.length > 0 ? filteredLogs[filteredLogs.length - 1].sequence : 0
  const lineNumWidth = Math.max(String(maxSeq).length, 3)

  const logStepGroups = stepGroups.filter((group) => group.logs.length > 0)
  const hasSteps = logStepGroups.length > 0

  return (
    <div className="flex h-[60dvh] min-h-96 max-h-[48rem] flex-col">
      {/* Toolbar */}
      <div className="flex shrink-0 flex-wrap items-center gap-2 border border-b-0 bg-muted/50 px-3 py-2">
        <div className="flex items-center gap-1.5">
          {isStreaming ? (
            <span className="flex items-center gap-1.5 text-xs text-muted-foreground">
              <span className="relative flex size-2">
                <span className="absolute inline-flex size-full bg-success opacity-75 motion-safe:animate-ping" />
                <span className="relative inline-flex size-2 bg-success" />
              </span>
              Live
            </span>
          ) : (
            <span className="text-xs text-muted-foreground">
              {filteredLogs.length} lines
            </span>
          )}
        </div>

        <div className="flex-1" />

        {searchOpen ? (
          <div className="flex min-w-48 flex-1 items-center gap-1 sm:max-w-64">
            <Input
              ref={searchInputRef}
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              placeholder="Search logs..."
              className="h-9 min-w-0 flex-1 font-mono text-xs"
            />
            <Button
              variant="ghost"
              size="icon-sm"
              className="max-md:size-11"
              aria-label="Close log search"
              onClick={() => {
                setSearchOpen(false)
                setSearchQuery('')
              }}
            >
              <HugeiconsIcon icon={Cancel01Icon} />
            </Button>
          </div>
        ) : (
          <Button
            variant="ghost"
            size="icon-sm"
            className="max-md:size-11"
            aria-label="Search logs"
            onClick={() => {
              setSearchOpen(true)
              setTimeout(() => searchInputRef.current?.focus(), 0)
            }}
            title="Search (Ctrl+F)"
          >
            <HugeiconsIcon icon={Search01Icon} />
          </Button>
        )}

        <Button
          variant="ghost"
          size="icon-sm"
          className="max-md:size-11"
          aria-label="Jump to first error"
          onClick={jumpToFirstError}
          title="Jump to first error"
        >
          <HugeiconsIcon icon={AlertCircleIcon} />
        </Button>

        <Button
          variant="ghost"
          size="icon-sm"
          className="text-muted-foreground aria-pressed:text-foreground max-md:size-11"
          aria-label="Toggle line wrapping"
          aria-pressed={wrapLines}
          onClick={() => setWrapLines((prev) => !prev)}
          title="Toggle word wrap"
        >
          <HugeiconsIcon icon={TextWrapIcon} />
        </Button>

        <Button
          variant="ghost"
          size="icon-sm"
          className="max-md:size-11"
          aria-label="Download raw logs"
          onClick={downloadRawLogs}
          title="Download raw logs"
        >
          <HugeiconsIcon icon={Download04Icon} />
        </Button>

        {!autoScroll && filteredLogs.length > 0 ? (
          <Button
            variant="ghost"
            size="icon-sm"
            className="max-md:size-11"
            aria-label="Scroll to latest log"
            onClick={() => {
              setAutoScroll(true)
              virtualizer.scrollToIndex(filteredLogs.length - 1, {
                align: 'end',
              })
            }}
            title="Scroll to bottom"
          >
            <HugeiconsIcon icon={ArrowDown01Icon} />
          </Button>
        ) : null}
      </div>

      {streamError ? (
        <Alert variant="destructive" className="rounded-none border-b-0 py-2">
          <HugeiconsIcon icon={AlertCircleIcon} />
          <AlertDescription>{streamError}</AlertDescription>
        </Alert>
      ) : null}

      {/* Main log area */}
      {/* Mobile step selector */}
      {hasSteps ? (
        <div className="border border-b-0 bg-muted/30 px-3 py-2 md:hidden">
          <Select
            aria-label="Build step"
            value={selectedStep}
            onValueChange={(value) => setUserSelectedStep(value ?? 'all')}
          >
            <SelectTrigger className="h-11 w-full" aria-label="Build step">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectGroup>
                <SelectItem value="all">
                  All logs ({allVisibleLogs.length})
                </SelectItem>
                {logStepGroups.map((group) => (
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
      ) : null}

      <div
        className={
          hasSteps
            ? 'grid min-h-0 flex-1 grid-cols-1 overflow-hidden border md:grid-cols-[220px_minmax(0,1fr)]'
            : 'min-h-0 flex-1 overflow-hidden border'
        }
      >
        {/* Step sidebar (desktop only) */}
        {hasSteps ? (
          <aside className="hidden flex-col overflow-y-auto border-r bg-muted/20 p-1 md:flex">
            <Button
              variant={selectedStep === 'all' ? 'secondary' : 'ghost'}
              size="xs"
              onClick={() => setUserSelectedStep('all')}
              className="h-auto w-full justify-start rounded-none px-2 py-1.5"
            >
              <span className="font-medium">All logs</span>
              <span className="ml-auto font-mono text-[10px] text-muted-foreground">
                {allVisibleLogs.length}
              </span>
            </Button>
            {logStepGroups.map((group) => (
              <Button
                key={group.name}
                variant={selectedStep === group.name ? 'secondary' : 'ghost'}
                size="xs"
                onClick={() => setUserSelectedStep(group.name)}
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
        ) : null}

        {/* Log content */}
        <div className="flex min-h-0 flex-col overflow-hidden bg-card">
          {/* Sticky step header */}
          {selectedStepMeta ? (
            <div className="flex shrink-0 items-center gap-3 border-b bg-muted/30 px-4 py-2">
              <Badge variant={stepStatusVariant(selectedStepMeta.status)}>
                {selectedStepMeta.status}
              </Badge>
              <span className="text-xs font-medium">{selectedStep}</span>
              {selectedStepMeta.command ? (
                <code className="font-mono text-[11px] text-muted-foreground">
                  $ {selectedStepMeta.command}
                </code>
              ) : null}
              {selectedStepMeta.durationMs != null ? (
                <span className="ml-auto font-mono text-[11px] text-muted-foreground">
                  {formatDuration(selectedStepMeta.durationMs / 1000)}
                </span>
              ) : null}
            </div>
          ) : null}

          {/* Virtualized log lines */}
          {filteredLogs.length === 0 ? (
            <div className="flex h-48 items-center justify-center">
              <span className="text-xs text-muted-foreground">
                {searchQuery
                  ? 'No matching lines'
                  : logsUnavailable
                    ? 'Logs are unavailable for this build.'
                    : isTerminal
                      ? 'This build completed without recorded logs.'
                      : 'No logs yet'}
              </span>
            </div>
          ) : (
            <ScrollArea
              className="min-h-0 flex-1"
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
                  const chunk = filteredLogs[virtualRow.index]
                  const isStderr = chunk.stream === 'stderr'
                  const isError = ERROR_PATTERN.test(chunk.content)
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
                      className={`flex font-mono text-[13px] leading-[20px] ${
                        isStderr
                          ? 'text-destructive'
                          : isError
                            ? 'bg-destructive/10 text-destructive'
                            : 'text-card-foreground'
                      }`}
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
      </div>
    </div>
  )
}

// ── Step status icon ─────────────────────────────────────────

function StepStatusIcon({ status }: { status: string }) {
  const normalized = status.trim().toLowerCase()
  if (normalized === 'running') {
    return (
      <HugeiconsIcon
        icon={Loading03Icon}
        className="shrink-0 animate-spin text-info"
      />
    )
  }
  if (normalized === 'succeeded') {
    return (
      <HugeiconsIcon
        icon={CheckmarkCircle02Icon}
        className="shrink-0 text-success"
      />
    )
  }
  if (
    normalized === 'failed' ||
    normalized === 'canceled' ||
    normalized === 'timed_out'
  ) {
    return (
      <HugeiconsIcon
        icon={AlertCircleIcon}
        className="shrink-0 text-destructive"
      />
    )
  }
  return (
    <span className="flex size-3 shrink-0 items-center justify-center">
      <span className="size-1.5 bg-muted-foreground" />
    </span>
  )
}
