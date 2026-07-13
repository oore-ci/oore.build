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
import {
  InputGroup,
  InputGroupAddon,
  InputGroupButton,
  InputGroupInput,
} from '@/components/ui/input-group'

interface LogToolbarProps {
  isStreaming: boolean
  logCount: number
  totalLogCount: number
  searchQuery: string
  searchInputRef: RefObject<HTMLInputElement | null>
  wrapLines: boolean
  showScrollLatest: boolean
  hasErrors: boolean
  onSearchQueryChange: (query: string) => void
  onSearchClear: () => void
  onJumpToError: () => void
  onToggleWrap: () => void
  onDownload: () => void
  onScrollLatest: () => void
}

export function LogToolbar({
  isStreaming,
  logCount,
  totalLogCount,
  searchQuery,
  searchInputRef,
  wrapLines,
  showScrollLatest,
  hasErrors,
  onSearchQueryChange,
  onSearchClear,
  onJumpToError,
  onToggleWrap,
  onDownload,
  onScrollLatest,
}: LogToolbarProps) {
  const lineCountLabel = searchQuery
    ? `${logCount} of ${totalLogCount} lines`
    : `${totalLogCount} lines`

  return (
    <div className="flex shrink-0 flex-wrap items-center gap-2">
      {isStreaming ? (
        <span className="flex items-center gap-1.5 text-xs text-muted-foreground">
          <span className="relative flex size-2">
            <span className="absolute inline-flex size-full bg-success opacity-75 motion-safe:animate-ping" />
            <span className="relative inline-flex size-2 bg-success" />
          </span>
          Live
        </span>
      ) : (
        <span className="text-xs text-muted-foreground">{lineCountLabel}</span>
      )}

      <div className="flex-1" />

      <InputGroup className="order-first w-full flex-none bg-background sm:order-none sm:w-auto sm:min-w-48 sm:flex-1 sm:max-w-64">
        <InputGroupInput
          ref={searchInputRef}
          value={searchQuery}
          onChange={(event) => onSearchQueryChange(event.target.value)}
          placeholder="Search logs"
          aria-label="Search build logs"
          className="font-mono text-xs"
        />
        <InputGroupAddon align="inline-start">
          <HugeiconsIcon icon={Search01Icon} />
        </InputGroupAddon>
        {searchQuery ? (
          <InputGroupAddon align="inline-end">
            <InputGroupButton
              size="icon-xs"
              aria-label="Clear log search"
              onClick={onSearchClear}
            >
              <HugeiconsIcon icon={Cancel01Icon} />
            </InputGroupButton>
          </InputGroupAddon>
        ) : null}
      </InputGroup>

      <ToolbarButton
        label="Jump to first error"
        title="Jump to first error"
        disabled={!hasErrors}
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
  disabled,
  onClick,
  children,
}: {
  label: string
  title?: string
  className?: string
  pressed?: boolean
  disabled?: boolean
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
      disabled={disabled}
      onClick={onClick}
      title={title}
    >
      {children}
    </Button>
  )
}
