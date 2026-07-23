import {
  CircleAlert as AlertCircleIcon,
  X as Cancel01Icon,
  Download as Download04Icon,
  Search as Search01Icon,
  WrapText as TextWrapIcon,
} from 'lucide-react'
import type { RefObject } from 'react'

import { Button } from '@/components/ui/button'
import {
  InputGroup,
  InputGroupAddon,
  InputGroupButton,
  InputGroupInput,
} from '@/components/ui/input-group'

interface LogToolbarProps {
  searchQuery: string
  searchInputRef: RefObject<HTMLInputElement | null>
  wrapLines: boolean
  hasErrors: boolean
  onSearchQueryChange: (query: string) => void
  onSearchClear: () => void
  onJumpToError: () => void
  onToggleWrap: () => void
  onDownload: () => void
}

export function LogToolbar({
  searchQuery,
  searchInputRef,
  wrapLines,
  hasErrors,
  onSearchQueryChange,
  onSearchClear,
  onJumpToError,
  onToggleWrap,
  onDownload,
}: LogToolbarProps) {
  return (
    <div className="flex w-full shrink-0 flex-wrap items-center justify-end gap-1.5">
      <InputGroup className="order-first w-full flex-none bg-background sm:order-0 sm:w-56">
        <InputGroupInput
          ref={searchInputRef}
          value={searchQuery}
          onChange={(event) => onSearchQueryChange(event.target.value)}
          placeholder="Search logs"
          aria-label="Search build logs"
          className="font-mono text-xs"
        />
        <InputGroupAddon align="inline-start">
          <Search01Icon />
        </InputGroupAddon>
        {searchQuery ? (
          <InputGroupAddon align="inline-end">
            <InputGroupButton
              size="icon-xs"
              aria-label="Clear log search"
              onClick={onSearchClear}
            >
              <Cancel01Icon />
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
        <AlertCircleIcon />
      </ToolbarButton>
      <ToolbarButton
        label="Toggle line wrapping"
        title="Toggle word wrap"
        className="text-muted-foreground aria-pressed:text-foreground max-md:size-11"
        pressed={wrapLines}
        onClick={onToggleWrap}
      >
        <TextWrapIcon />
      </ToolbarButton>
      <ToolbarButton
        label="Download raw logs"
        title="Download raw logs"
        onClick={onDownload}
      >
        <Download04Icon />
      </ToolbarButton>
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
