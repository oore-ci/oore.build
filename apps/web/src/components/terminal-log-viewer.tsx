import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
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
import { parseAnsi } from '@/lib/ansi-to-html'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { ScrollArea } from '@/components/ui/scroll-area'

// ── Types ──────────────────────────────────────────────────────

export interface TerminalLogViewerProps {
  logs: Array<BuildLogChunk>
  stepResults: Array<StepResult>
  isStreaming: boolean
  streamError?: string
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
          <span key={i} style={style}>{span.text}</span>
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
}: TerminalLogViewerProps) {
  const [selectedStep, setSelectedStep] = useState<string>('')
  const [autoScroll, setAutoScroll] = useState(true)
  const [wrapLines, setWrapLines] = useState(false)
  const [searchQuery, setSearchQuery] = useState('')
  const [searchOpen, setSearchOpen] = useState(false)
  const scrollContainerRef = useRef<HTMLDivElement>(null)
  const searchInputRef = useRef<HTMLInputElement>(null)

  // ── Step grouping ──────────────────────────────────────────

  const { stepGroups, allVisibleLogs } = useMemo(() => {
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

    return {
      stepGroups: order
        .map((name) => groups.get(name))
        .filter(Boolean) as Array<StepGroup>,
      allVisibleLogs: visibleLogs,
    }
  }, [logs, stepResults])

  // ── Auto-select running step ───────────────────────────────

  useEffect(() => {
    const hasSelected =
      selectedStep === 'all' || stepGroups.some((g) => g.name === selectedStep)
    if (hasSelected) return
    const running = stepGroups.find((g) => g.status === 'running')
    setSelectedStep(running?.name ?? stepGroups.at(0)?.name ?? 'all')
  }, [stepGroups, selectedStep])

  // ── Filtered logs ──────────────────────────────────────────

  const selectedLogs = useMemo(() => {
    if (selectedStep === 'all') return allVisibleLogs
    return stepGroups.find((g) => g.name === selectedStep)?.logs ?? []
  }, [selectedStep, allVisibleLogs, stepGroups])

  const filteredLogs = useMemo(() => {
    if (!searchQuery.trim()) return selectedLogs
    const q = searchQuery.toLowerCase()
    return selectedLogs.filter((chunk) =>
      chunk.content.toLowerCase().includes(q),
    )
  }, [selectedLogs, searchQuery])

  const selectedStepMeta = useMemo(() => {
    if (selectedStep === 'all') return null
    const group = stepGroups.find((g) => g.name === selectedStep)
    if (!group) return null
    return {
      command: group.command,
      status: group.status,
      durationMs: group.durationMs,
    }
  }, [selectedStep, stepGroups])

  // ── Virtualizer ────────────────────────────────────────────

  const virtualizer = useVirtualizer({
    count: filteredLogs.length,
    getScrollElement: () => scrollContainerRef.current,
    estimateSize: () => 20,
    overscan: 50,
  })

  // ── Auto-scroll ────────────────────────────────────────────

  useEffect(() => {
    if (autoScroll && filteredLogs.length > 0) {
      virtualizer.scrollToIndex(filteredLogs.length - 1, { align: 'end' })
    }
  }, [filteredLogs.length, autoScroll, virtualizer])

  const handleScroll = useCallback(() => {
    const el = scrollContainerRef.current
    if (!el) return
    const isAtBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 40
    setAutoScroll(isAtBottom)
  }, [])

  // Attach scroll listener to the ScrollArea viewport
  useEffect(() => {
    const el = scrollContainerRef.current
    if (!el) return
    el.addEventListener('scroll', handleScroll)
    return () => el.removeEventListener('scroll', handleScroll)
  }, [handleScroll])

  // ── Keyboard shortcuts ─────────────────────────────────────

  useEffect(() => {
    function handleKeyDown(e: KeyboardEvent) {
      if ((e.ctrlKey || e.metaKey) && e.key === 'f') {
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
      if (e.key === 'Escape' && searchOpen) {
        setSearchOpen(false)
        setSearchQuery('')
      }
    }
    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [searchOpen])

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

  const hasSteps = stepGroups.length > 0

  return (
    <div className="flex h-[calc(100vh-18rem)] min-h-80 flex-col">
      {/* Toolbar */}
      <div className="flex shrink-0 flex-wrap items-center gap-2 border border-b-0 border-border/60 bg-[oklch(0.18_0_0)] px-3 py-2">
        <div className="flex items-center gap-1.5">
          {isStreaming ? (
            <span className="flex items-center gap-1.5 text-xs text-[oklch(0.75_0_0)]">
              <span className="relative flex size-2">
                <span className="absolute inline-flex size-full animate-ping rounded-full bg-[oklch(0.8_0.15_145)] opacity-75" />
                <span className="relative inline-flex size-2 rounded-full bg-[oklch(0.75_0.15_145)]" />
              </span>
              Live
            </span>
          ) : (
            <span className="text-xs text-[oklch(0.72_0_0)]">
              {filteredLogs.length} lines
            </span>
          )}
        </div>

        <div className="flex-1" />

        {streamError ? (
          <span className="text-xs text-[oklch(0.80_0.15_25)]">
            {streamError}
          </span>
        ) : null}

        {searchOpen ? (
          <div className="flex items-center gap-1">
            <Input
              ref={searchInputRef}
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              placeholder="Search logs..."
              className="h-7 w-32 sm:w-48 border-[oklch(0.30_0_0)] bg-[oklch(0.14_0_0)] font-mono text-xs text-[oklch(0.92_0_0)] placeholder:text-[oklch(0.48_0_0)]"
            />
            <Button
              variant="ghost"
              size="sm"
              className="h-7 w-7 p-0 text-[oklch(0.80_0_0)] hover:bg-[oklch(0.25_0_0)] hover:text-[oklch(0.95_0_0)]"
              onClick={() => {
                setSearchOpen(false)
                setSearchQuery('')
              }}
            >
              <HugeiconsIcon icon={Cancel01Icon} size={12} />
            </Button>
          </div>
        ) : (
          <Button
            variant="ghost"
            size="sm"
            className="h-7 px-2 text-[oklch(0.80_0_0)] hover:bg-[oklch(0.25_0_0)] hover:text-[oklch(0.95_0_0)]"
            onClick={() => {
              setSearchOpen(true)
              setTimeout(() => searchInputRef.current?.focus(), 0)
            }}
            title="Search (Ctrl+F)"
          >
            <HugeiconsIcon icon={Search01Icon} size={13} />
          </Button>
        )}

        <Button
          variant="ghost"
          size="sm"
          className="h-7 px-2 text-[oklch(0.80_0_0)] hover:bg-[oklch(0.25_0_0)] hover:text-[oklch(0.95_0_0)]"
          onClick={jumpToFirstError}
          title="Jump to first error"
        >
          <HugeiconsIcon icon={AlertCircleIcon} size={13} />
        </Button>

        <Button
          variant="ghost"
          size="sm"
          className={`h-7 px-2 ${wrapLines ? 'text-[oklch(0.95_0_0)]' : 'text-[oklch(0.80_0_0)]'} hover:bg-[oklch(0.25_0_0)] hover:text-[oklch(0.95_0_0)]`}
          onClick={() => setWrapLines((prev) => !prev)}
          title="Toggle word wrap"
        >
          <HugeiconsIcon icon={TextWrapIcon} size={13} />
        </Button>

        <Button
          variant="ghost"
          size="sm"
          className="h-7 px-2 text-[oklch(0.80_0_0)] hover:bg-[oklch(0.25_0_0)] hover:text-[oklch(0.95_0_0)]"
          onClick={downloadRawLogs}
          title="Download raw logs"
        >
          <HugeiconsIcon icon={Download04Icon} size={13} />
        </Button>

        {!autoScroll && filteredLogs.length > 0 ? (
          <Button
            variant="ghost"
            size="sm"
            className="h-7 px-2 text-[oklch(0.80_0_0)] hover:bg-[oklch(0.25_0_0)] hover:text-[oklch(0.95_0_0)]"
            onClick={() => {
              setAutoScroll(true)
              virtualizer.scrollToIndex(filteredLogs.length - 1, {
                align: 'end',
              })
            }}
            title="Scroll to bottom"
          >
            <HugeiconsIcon icon={ArrowDown01Icon} size={13} />
          </Button>
        ) : null}
      </div>

      {/* Main log area */}
      {/* Mobile step selector */}
      {hasSteps ? (
        <div className="flex items-center gap-2 border border-b-0 border-[oklch(0.25_0_0)] bg-[oklch(0.17_0_0)] px-3 py-2 md:hidden">
          <select
            value={selectedStep}
            onChange={(e) => setSelectedStep(e.target.value)}
            className="w-full bg-[oklch(0.14_0_0)] text-[oklch(0.92_0_0)] text-xs border border-[oklch(0.30_0_0)] px-2 py-1.5"
          >
            <option value="all">All logs ({allVisibleLogs.length})</option>
            {stepGroups.map((group) => (
              <option key={group.name} value={group.name}>
                {group.status === 'running'
                  ? '● '
                  : group.status === 'succeeded'
                    ? '✓ '
                    : group.status === 'failed' ||
                        group.status === 'canceled' ||
                        group.status === 'timed_out'
                      ? '✗ '
                      : '○ '}
                {group.name}
                {group.durationMs != null
                  ? ` (${formatDuration(group.durationMs / 1000)})`
                  : ''}
              </option>
            ))}
          </select>
        </div>
      ) : null}

      <div
        className={
          hasSteps
            ? 'grid min-h-0 flex-1 grid-cols-1 md:grid-cols-[220px_minmax(0,1fr)] overflow-hidden border border-[oklch(0.25_0_0)]'
            : 'min-h-0 flex-1 overflow-hidden border border-[oklch(0.25_0_0)]'
        }
      >
        {/* Step sidebar (desktop only) */}
        {hasSteps ? (
          <aside className="hidden md:flex flex-col gap-0 overflow-y-auto border-r border-[oklch(0.25_0_0)] bg-[oklch(0.15_0_0)] py-1">
            <button
              type="button"
              onClick={() => setSelectedStep('all')}
              className={`flex items-center justify-between px-3 py-1.5 text-left text-xs transition-colors ${
                selectedStep === 'all'
                  ? 'border-l-2 border-l-[oklch(0.77_0.16_70)] bg-[oklch(0.20_0_0)] text-[oklch(0.95_0_0)]'
                  : 'border-l-2 border-l-transparent text-[oklch(0.78_0_0)] hover:bg-[oklch(0.19_0_0)] hover:text-[oklch(0.90_0_0)]'
              }`}
            >
              <span className="font-medium">All logs</span>
              <span className="font-mono text-[10px] text-[oklch(0.65_0_0)]">
                {allVisibleLogs.length}
              </span>
            </button>
            {stepGroups.map((group) => (
              <button
                key={group.name}
                type="button"
                onClick={() => setSelectedStep(group.name)}
                className={`flex items-center gap-2 px-3 py-1.5 text-left text-xs transition-colors ${
                  selectedStep === group.name
                    ? 'border-l-2 border-l-[oklch(0.77_0.16_70)] bg-[oklch(0.20_0_0)] text-[oklch(0.95_0_0)]'
                    : 'border-l-2 border-l-transparent text-[oklch(0.78_0_0)] hover:bg-[oklch(0.19_0_0)] hover:text-[oklch(0.90_0_0)]'
                }`}
                title={group.command}
              >
                <StepStatusIcon status={group.status} />
                <span className="min-w-0 flex-1 truncate">{group.name}</span>
                {group.durationMs != null ? (
                  <span className="shrink-0 font-mono text-[10px] text-[oklch(0.65_0_0)]">
                    {formatDuration(group.durationMs / 1000)}
                  </span>
                ) : null}
              </button>
            ))}
          </aside>
        ) : null}

        {/* Log content */}
        <div className="flex min-h-0 flex-col overflow-hidden bg-[oklch(0.14_0_0)]">
          {/* Sticky step header */}
          {selectedStepMeta ? (
            <div className="flex shrink-0 items-center gap-3 border-b border-[oklch(0.25_0_0)] bg-[oklch(0.17_0_0)] px-4 py-2">
              <Badge variant={stepStatusVariant(selectedStepMeta.status)}>
                {selectedStepMeta.status}
              </Badge>
              <span className="text-xs font-medium text-[oklch(0.92_0_0)]">
                {selectedStep}
              </span>
              {selectedStepMeta.command ? (
                <code className="font-mono text-[11px] text-[oklch(0.78_0_0)]">
                  $ {selectedStepMeta.command}
                </code>
              ) : null}
              {selectedStepMeta.durationMs != null ? (
                <span className="ml-auto font-mono text-[11px] text-[oklch(0.72_0_0)]">
                  {formatDuration(selectedStepMeta.durationMs / 1000)}
                </span>
              ) : null}
            </div>
          ) : null}

          {/* Virtualized log lines */}
          {filteredLogs.length === 0 ? (
            <div className="flex h-48 items-center justify-center">
              <span className="text-xs text-[oklch(0.52_0_0)]">
                {searchQuery ? 'No matching lines' : 'No logs yet'}
              </span>
            </div>
          ) : (
            <ScrollArea
              className="min-h-0 flex-1"
              viewportRef={scrollContainerRef}
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
                          ? 'text-[oklch(0.80_0.15_25)]'
                          : isError
                            ? 'text-[oklch(0.80_0.15_25)] bg-[oklch(0.80_0.15_25/0.06)]'
                            : 'text-[oklch(0.88_0_0)]'
                      }`}
                    >
                      <span
                        className="shrink-0 select-none text-right text-[oklch(0.48_0_0)] pr-3 pl-3"
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
        size={12}
        className="shrink-0 animate-spin text-[oklch(0.72_0.14_250)]"
      />
    )
  }
  if (normalized === 'succeeded') {
    return (
      <HugeiconsIcon
        icon={CheckmarkCircle02Icon}
        size={12}
        className="shrink-0 text-[oklch(0.75_0.17_149)]"
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
        size={12}
        className="shrink-0 text-[oklch(0.80_0.15_25)]"
      />
    )
  }
  return (
    <span className="flex size-3 shrink-0 items-center justify-center">
      <span className="size-1.5 rounded-full bg-[oklch(0.48_0_0)]" />
    </span>
  )
}
