import { HugeiconsIcon } from '@hugeicons/react'
import {
  ArrowDown01Icon,
  ArrowUp01Icon,
  Cancel01Icon,
  Copy01Icon,
  Download04Icon,
  Loading03Icon,
  Search01Icon,
  TextWrapIcon,
} from '@hugeicons/core-free-icons'
import type { RefObject } from 'react'

import { Button } from '@/components/ui/button'
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from '@/components/ui/tooltip'
import { PAGE_SEARCH } from '@/lib/keyboard-shortcuts'
import {
  InputGroup,
  InputGroupAddon,
  InputGroupButton,
  InputGroupInput,
  InputGroupText,
} from '@/components/ui/input-group'
import { Row } from '@/components/layout/row'

interface LogToolbarProps {
  searchQuery: string
  searchInputRef: RefObject<HTMLInputElement | null>
  matchCount: number
  activeMatchPosition: number
  wrapLines: boolean
  followLive: boolean
  /** The build is unfinished, so following new lines makes sense. */
  isLive: boolean
  /** Copy and Search cover the selected step or the whole log. */
  scope: 'log' | 'step'
  hasLines: boolean
  hasAnyLines: boolean
  onSearchQueryChange: (query: string) => void
  onSearchClear: () => void
  onPreviousMatch: () => void
  onNextMatch: () => void
  onToggleWrap: () => void
  onToggleFollow: () => void
  onCopy: () => void
  onDownload: () => void
}

export function LogToolbar({
  searchQuery,
  searchInputRef,
  matchCount,
  activeMatchPosition,
  wrapLines,
  followLive,
  isLive,
  scope,
  hasLines,
  hasAnyLines,
  onSearchQueryChange,
  onSearchClear,
  onPreviousMatch,
  onNextMatch,
  onToggleWrap,
  onToggleFollow,
  onCopy,
  onDownload,
}: LogToolbarProps) {
  return (
    <Row
      gap={1.5}
      className="w-full min-w-0 shrink-0 max-sm:flex-wrap sm:justify-end"
    >
      <InputGroup className="w-full border-input bg-background sm:mr-auto sm:w-auto sm:max-w-72 sm:min-w-36 sm:flex-1">
        <InputGroupInput
          ref={searchInputRef}
          {...PAGE_SEARCH}
          value={searchQuery}
          onChange={(event) => onSearchQueryChange(event.target.value)}
          placeholder={scope === 'step' ? 'Search this step' : 'Search log'}
          aria-label={
            scope === 'step'
              ? 'Search the selected step'
              : 'Search the build log'
          }
          className="text-xs"
          disabled={!hasLines}
        />
        <InputGroupAddon align="inline-start">
          <HugeiconsIcon icon={Search01Icon} />
        </InputGroupAddon>
        {searchQuery ? (
          <InputGroupAddon align="inline-end" className="gap-0">
            <InputGroupText className="mr-1 font-mono text-[10px] tabular-nums">
              {matchCount === 0
                ? '0 matches'
                : `${activeMatchPosition}/${matchCount}`}
            </InputGroupText>
            <InputGroupButton
              size="icon-xs"
              aria-label="Previous log match"
              disabled={matchCount === 0}
              onClick={onPreviousMatch}
            >
              <HugeiconsIcon icon={ArrowUp01Icon} />
            </InputGroupButton>
            <InputGroupButton
              size="icon-xs"
              aria-label="Next log match"
              disabled={matchCount === 0}
              onClick={onNextMatch}
            >
              <HugeiconsIcon icon={ArrowDown01Icon} />
            </InputGroupButton>
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

      <ToolbarAction
        label="Wrap"
        description="Wrap long lines"
        icon={TextWrapIcon}
        pressed={wrapLines}
        disabled={!hasLines}
        onClick={onToggleWrap}
      />
      {isLive ? (
        <ToolbarAction
          label="Follow"
          description="Follow new lines"
          icon={Loading03Icon}
          pressed={followLive}
          onClick={onToggleFollow}
        />
      ) : null}
      <ToolbarAction
        label="Copy"
        description={scope === 'step' ? 'Copy this step' : 'Copy the log'}
        icon={Copy01Icon}
        disabled={!hasLines}
        onClick={onCopy}
      />
      <ToolbarAction
        label="Download"
        description="Download the full raw log"
        icon={Download04Icon}
        disabled={!hasAnyLines}
        onClick={onDownload}
      />
    </Row>
  )
}

/** Labelled on wider screens; icon-only with a tooltip on phones. */
function ToolbarAction({
  label,
  description,
  icon,
  pressed,
  disabled,
  onClick,
}: {
  label: string
  description: string
  icon: typeof Copy01Icon
  pressed?: boolean
  disabled?: boolean
  onClick: () => void
}) {
  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <Button
            variant="outline"
            size="sm"
            aria-label={description}
            aria-pressed={pressed}
            disabled={disabled}
            onClick={onClick}
            className="aria-pressed:bg-muted max-sm:size-11 max-sm:px-0"
          />
        }
      >
        <HugeiconsIcon icon={icon} data-icon="inline-start" />
        <span className="max-sm:sr-only">{label}</span>
      </TooltipTrigger>
      <TooltipContent>{description}</TooltipContent>
    </Tooltip>
  )
}
