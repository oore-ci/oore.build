import { HugeiconsIcon } from '@hugeicons/react'
import { Alert02Icon, AlertCircleIcon } from '@hugeicons/core-free-icons'
import type { UseQueryResult } from '@tanstack/react-query'
import type { RuntimeUpdatePreflight } from '@/hooks/use-runtime-updates'
import { cn } from '@/lib/utils'
import {
  Item,
  ItemContent,
  ItemDescription,
  ItemGroup,
  ItemMedia,
  ItemTitle,
} from '@/components/ui/item'
import { textVariants } from '@/lib/recipes'

function withInlineCode(text: string) {
  return text.split('`').map((part, index) =>
    index % 2 === 1 ? (
      <code key={index} className="font-mono text-[11px]">
        {part}
      </code>
    ) : (
      part
    ),
  )
}

/**
 * Lists the backend update preflight checks that did not pass, so a disabled
 * update button always comes with the failing check and what to run.
 */
export function RuntimeUpdatePreflightChecks({
  className,
  preflight,
}: {
  className?: string
  preflight: UseQueryResult<RuntimeUpdatePreflight>
}) {
  if (preflight.isError) {
    return (
      <p className={cn(textVariants({ variant: 'caption' }), className)}>
        Could not check whether this backend can update from the web:{' '}
        {preflight.error.message}. Run{' '}
        <code className="font-mono text-[11px]">oore doctor --update</code> on
        the backend Mac.
      </p>
    )
  }
  const checks =
    preflight.data?.checks.filter((check) => check.status !== 'pass') ?? []
  if (checks.length === 0) return null

  return (
    <ItemGroup
      aria-label="Backend update checks"
      className={cn('gap-1', className)}
    >
      {checks.map((check) => (
        <Item
          key={check.id}
          size="xs"
          variant="muted"
          className="flex-nowrap items-start"
        >
          <ItemMedia variant="icon">
            <HugeiconsIcon
              icon={check.status === 'fail' ? AlertCircleIcon : Alert02Icon}
              className={
                check.status === 'fail' ? 'text-destructive' : 'text-warning'
              }
              aria-label={check.status === 'fail' ? 'Failed' : 'Warning'}
            />
          </ItemMedia>
          <ItemContent className="min-w-0">
            <ItemTitle>{check.label}</ItemTitle>
            {check.detail ? (
              <ItemDescription className="line-clamp-none wrap-anywhere">
                {withInlineCode(check.detail)}
              </ItemDescription>
            ) : null}
            {check.fix ? (
              <ItemDescription className="line-clamp-none wrap-anywhere text-foreground">
                {withInlineCode(check.fix)}
              </ItemDescription>
            ) : null}
          </ItemContent>
        </Item>
      ))}
    </ItemGroup>
  )
}

export function backendUpdateReady(
  preflight: UseQueryResult<RuntimeUpdatePreflight>,
  managedService: boolean | undefined,
) {
  return preflight.data?.ready ?? managedService === true
}
