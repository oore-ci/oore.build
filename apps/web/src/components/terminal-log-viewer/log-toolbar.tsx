import {
  AlertCircleIcon,
  ArrowDown01Icon,
  Cancel01Icon,
  Download04Icon,
  Search01Icon,
  TextWrapIcon,
} from '@hugeicons/core-free-icons'
import { HugeiconsIcon } from '@hugeicons/react'
import type { RefObject } from 'react'

import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'

interface LogToolbarProps {
  isStreaming: boolean
  logCount: number
  searchOpen: boolean
  searchQuery: string
  searchInputRef: RefObject<HTMLInputElement | null>
  wrapLines: boolean
  showScrollLatest: boolean
  onSearchOpen: () => void
  onSearchClose: () => void
  onSearchQueryChange: (query: string) => void
  onJumpToError: () => void
  onToggleWrap: () => void
  onDownload: () => void
  onScrollLatest: () => void
}

export function LogToolbar({
  isStreaming,
  logCount,
  searchOpen,
  searchQuery,
  searchInputRef,
  wrapLines,
  showScrollLatest,
  onSearchOpen,
  onSearchClose,
  onSearchQueryChange,
  onJumpToError,
  onToggleWrap,
  onDownload,
  onScrollLatest,
}: LogToolbarProps) {
  return (
    <div className="flex shrink-0 flex-wrap items-center gap-2 border border-b-0 bg-muted/50 px-3 py-2">
      {isStreaming ? (
        <span className="flex items-center gap-1.5 text-xs text-muted-foreground">
          <span className="relative flex size-2">
            <span className="absolute inline-flex size-full bg-success opacity-75 motion-safe:animate-ping" />
            <span className="relative inline-flex size-2 bg-success" />
          </span>
          Live
        </span>
      ) : (
        <span className="text-xs text-muted-foreground">{logCount} lines</span>
      )}

      <div className="flex-1" />

      {searchOpen ? (
        <div className="flex min-w-48 flex-1 items-center gap-1 sm:max-w-64">
          <Input
            ref={searchInputRef}
            value={searchQuery}
            onChange={(event) => onSearchQueryChange(event.target.value)}
            placeholder="Search logs..."
            className="h-9 min-w-0 flex-1 font-mono text-xs"
          />
          <ToolbarButton label="Close log search" onClick={onSearchClose}>
            <HugeiconsIcon icon={Cancel01Icon} />
          </ToolbarButton>
        </div>
      ) : (
        <ToolbarButton
          label="Search logs"
          title="Search (Ctrl+F)"
          onClick={onSearchOpen}
        >
          <HugeiconsIcon icon={Search01Icon} />
        </ToolbarButton>
      )}

      <ToolbarButton
        label="Jump to first error"
        title="Jump to first error"
        onClick={onJumpToError}
      >
        <HugeiconsIcon icon={AlertCircleIcon} />
      </ToolbarButton>
      <ToolbarButton
        label="Toggle line wrapping"
        title="Toggle word wrap"
        className="text-muted-foreground aria-pressed:text-foreground max-md:size-11"
        pressed={wrapLines}
        onClick={onToggleWrap}
      >
        <HugeiconsIcon icon={TextWrapIcon} />
      </ToolbarButton>
      <ToolbarButton
        label="Download raw logs"
        title="Download raw logs"
        onClick={onDownload}
      >
        <HugeiconsIcon icon={Download04Icon} />
      </ToolbarButton>
      {showScrollLatest ? (
        <ToolbarButton
          label="Scroll to latest log"
          title="Scroll to bottom"
          onClick={onScrollLatest}
        >
          <HugeiconsIcon icon={ArrowDown01Icon} />
        </ToolbarButton>
      ) : null}
    </div>
  )
}

function ToolbarButton({
  label,
  title,
  className = 'max-md:size-11',
  pressed,
  onClick,
  children,
}: {
  label: string
  title?: string
  className?: string
  pressed?: boolean
  onClick: () => void
  children: React.ReactNode
}) {
  return (
    <Button
      variant="ghost"
      size="icon-sm"
      className={className}
      aria-label={label}
      aria-pressed={pressed}
      onClick={onClick}
      title={title}
    >
      {children}
    </Button>
  )
}
