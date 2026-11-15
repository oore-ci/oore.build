import { useState, type ReactNode } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import { ArrowDown01Icon } from '@hugeicons/core-free-icons'

import { cn } from '@/lib/utils'
import { Button } from '@/components/ui/button'
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible'
import { Stack } from '@/components/layout/stack'

/**
 * A closed "Advanced" fold on a store card. It starts open when one of its
 * options is changed from the default, and opens by itself when one of them
 * has an error, so a failed save never hides the field to fix.
 */
export function AdvancedOptions({
  changed,
  children,
  hasError,
  summary,
}: {
  /** True when an option inside differs from its default. */
  changed: boolean
  children: ReactNode
  hasError: boolean
  /** What is set inside, shown beside the closed trigger. */
  summary?: string
}) {
  const [userOpen, setUserOpen] = useState<boolean | null>(null)
  const open = hasError || (userOpen ?? changed)

  return (
    <Collapsible open={open} onOpenChange={setUserOpen}>
      <div className="flex flex-wrap items-center gap-x-2">
        <CollapsibleTrigger
          render={
            <Button type="button" variant="ghost" size="sm" className="-ml-2" />
          }
        >
          Advanced
          <HugeiconsIcon
            icon={ArrowDown01Icon}
            data-icon="inline-end"
            className={cn('transition-transform', open && 'rotate-180')}
          />
        </CollapsibleTrigger>
        {!open && summary ? (
          <span className="text-sm text-muted-foreground">{summary}</span>
        ) : null}
      </div>
      <CollapsibleContent>
        <Stack gap={4} className="pt-3">
          {children}
        </Stack>
      </CollapsibleContent>
    </Collapsible>
  )
}
