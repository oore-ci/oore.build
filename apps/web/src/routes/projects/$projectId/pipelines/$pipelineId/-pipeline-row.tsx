import type { ReactNode } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import { ArrowDown01Icon } from '@hugeicons/core-free-icons'

import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible'
import { cn } from '@/lib/utils'

export const PIPELINE_ROWS = [
  'source',
  'build',
  'signing',
  'publishing',
  'secrets',
] as const
export type PipelineRowKey = (typeof PIPELINE_ROWS)[number]

/** Old Edit page anchors and tab names, and the row that now holds them. */
const ROW_FOR_ANCHOR = new Map<string, PipelineRowKey>([
  ['basics', 'source'],
  ['triggers', 'source'],
  ['workflows', 'source'],
  ['build', 'build'],
  ['plan', 'build'],
  ['size-budget', 'build'],
  ['signing', 'signing'],
  ['android-signing', 'signing'],
  ['ios-signing', 'signing'],
  ['publishing', 'publishing'],
  ['google-play', 'publishing'],
  ['app-store', 'publishing'],
  ['secrets', 'secrets'],
])

export function rowForAnchor(anchor?: string): PipelineRowKey | undefined {
  if (!anchor) return undefined
  return ROW_FOR_ANCHOR.get(anchor.replace(/^#/, ''))
}

/** The bordered list the five rows sit in. */
export function PipelineRowList({ children }: { children: ReactNode }) {
  return (
    <div className="flex flex-col divide-y overflow-clip rounded-lg border bg-card text-card-foreground">
      {children}
    </div>
  )
}

/**
 * One row of the pipeline page: what is in force in one line, with where
 * it comes from, opening in place.
 */
export function PipelineRow({
  children,
  id,
  onOpenChange,
  open,
  sources,
  summary,
  title,
}: {
  children: ReactNode
  id: PipelineRowKey
  onOpenChange: (open: boolean) => void
  open: boolean
  /** Where the values come from, such as `from .oore.yaml · set here`. */
  sources: Array<string>
  summary: ReactNode
  title: string
}) {
  return (
    <Collapsible open={open} onOpenChange={onOpenChange}>
      <section
        id={`row-${id}`}
        aria-label={title}
        className="scroll-mt-[calc(var(--header-height)+1rem)]"
      >
        <CollapsibleTrigger className="block w-full text-left outline-none hover:bg-muted/40 focus-visible:bg-muted/40">
          <span className="flex items-start gap-3 px-4 py-3.5 sm:px-5">
            <span className="flex min-w-0 flex-1 flex-col gap-0.5 sm:flex-row sm:items-baseline sm:gap-4">
              <span className="shrink-0 text-sm font-medium sm:w-40">
                {title}
              </span>
              <span className="min-w-0 flex-1 truncate text-sm text-muted-foreground max-sm:whitespace-normal">
                {summary}
              </span>
              {sources.length > 0 ? (
                <span className="shrink-0 text-[13px] text-muted-foreground sm:text-right">
                  {sources.join(' · ')}
                </span>
              ) : null}
            </span>
            <HugeiconsIcon
              icon={ArrowDown01Icon}
              size={16}
              className={cn(
                'mt-0.5 shrink-0 text-muted-foreground transition-transform',
                open && 'rotate-180',
              )}
            />
          </span>
        </CollapsibleTrigger>
        <CollapsibleContent>
          <div className="border-t p-4 sm:p-5">{children}</div>
        </CollapsibleContent>
      </section>
    </Collapsible>
  )
}
