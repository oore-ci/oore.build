import { useState } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  Alert02Icon,
  AlertCircleIcon,
  CheckmarkCircle02Icon,
  Copy01Icon,
  Tick02Icon,
} from '@hugeicons/core-free-icons'
import type { Runner, RunnerToolchainCheck } from '@oore/client/models'
import { Button } from '@/components/ui/button'
import {
  Item,
  ItemContent,
  ItemDescription,
  ItemGroup,
  ItemMedia,
  ItemTitle,
} from '@/components/ui/item'
import {
  Popover,
  PopoverContent,
  PopoverDescription,
  PopoverHeader,
  PopoverTitle,
  PopoverTrigger,
} from '@/components/ui/popover'
import { Stack } from '@/components/layout/stack'
import {
  PLATFORM_GROUPS,
  checksFor,
  runnerPlatformState,
} from '@/lib/runner-toolchain'
import { cn } from '@/lib/utils'

/** A fix that ends a sentence is advice; anything else is a command. */
function isCommand(fix: string) {
  return !/[.)]$/.test(fix.trim())
}

function CopyCommand({ command }: { command: string }) {
  const [copied, setCopied] = useState(false)
  return (
    <div className="flex items-center gap-1 rounded-md bg-muted px-2 py-1">
      <code className="min-w-0 flex-1 font-mono text-[11px] wrap-anywhere text-foreground">
        {command}
      </code>
      <Button
        type="button"
        variant="ghost"
        size="icon-xs"
        aria-label={copied ? 'Copied' : `Copy ${command}`}
        onClick={() =>
          void navigator.clipboard.writeText(command).then(() => {
            setCopied(true)
            setTimeout(() => setCopied(false), 2000)
          })
        }
      >
        <HugeiconsIcon icon={copied ? Tick02Icon : Copy01Icon} />
      </Button>
    </div>
  )
}

/** Build tool checks that did not pass, each with its fix. */
export function ToolchainChecks({
  checks,
  className,
}: {
  checks: ReadonlyArray<RunnerToolchainCheck>
  className?: string
}) {
  if (checks.length === 0) return null
  return (
    <ItemGroup aria-label="Build tools" className={cn('gap-1', className)}>
      {checks.map((check) => (
        <Item
          key={check.id}
          size="xs"
          variant="muted"
          className="flex-nowrap items-start"
        >
          <ItemMedia variant="icon">
            <HugeiconsIcon
              icon={check.status === 'missing' ? AlertCircleIcon : Alert02Icon}
              className={
                check.status === 'missing' ? 'text-destructive' : 'text-warning'
              }
              aria-label={check.status === 'missing' ? 'Missing' : 'Warning'}
            />
          </ItemMedia>
          <ItemContent className="min-w-0 gap-1">
            <ItemTitle>
              {check.label}
              {check.status === 'missing' ? ' is missing' : ''}
            </ItemTitle>
            {check.detail ? (
              <ItemDescription className="line-clamp-none wrap-anywhere">
                {check.detail}
              </ItemDescription>
            ) : null}
            {check.fix ? (
              isCommand(check.fix) ? (
                <CopyCommand command={check.fix} />
              ) : (
                <ItemDescription className="line-clamp-none wrap-anywhere text-foreground">
                  {check.fix}
                </ItemDescription>
              )
            ) : null}
          </ItemContent>
        </Item>
      ))}
    </ItemGroup>
  )
}

/**
 * What a runner can build, per platform, for the runners table. A platform
 * it cannot build opens the missing tools and their fixes.
 */
export function RunnerToolchainCell({ runner }: { runner: Runner }) {
  if (!runner.toolchain?.length) {
    return <span className="text-muted-foreground">Not reported</span>
  }
  return (
    <Stack gap={1} className="items-start">
      {PLATFORM_GROUPS.map(({ label, platform }) => {
        const state = runnerPlatformState(runner, platform)
        if (state === 'ready') {
          return (
            <span key={platform} className="flex items-center gap-1 text-sm">
              <HugeiconsIcon
                icon={CheckmarkCircle02Icon}
                className="size-3.5 text-success"
                aria-hidden
              />
              {label}
            </span>
          )
        }
        const problems = checksFor(runner, platform).filter(
          (check) => check.status !== 'ok',
        )
        const missing = problems.filter((check) => check.status === 'missing')
        return (
          <Popover key={platform}>
            <PopoverTrigger
              render={
                <Button
                  variant="link"
                  size="xs"
                  className="h-auto px-0 text-muted-foreground"
                />
              }
            >
              {label}: needs {missing.map((check) => check.label).join(', ')}
            </PopoverTrigger>
            <PopoverContent align="start" className="w-96 max-w-[90vw]">
              <PopoverHeader>
                <PopoverTitle>
                  {runner.name} cannot build {label} yet
                </PopoverTitle>
                <PopoverDescription>
                  Run these on the runner&apos;s Mac, as the account the runner
                  runs as. The runner checks again every 15 minutes.
                </PopoverDescription>
              </PopoverHeader>
              <ToolchainChecks checks={problems} />
            </PopoverContent>
          </Popover>
        )
      })}
    </Stack>
  )
}
