import { HugeiconsIcon } from '@hugeicons/react'
import { ArrowDown01Icon, ArrowUp01Icon } from '@hugeicons/core-free-icons'

import { Badge } from '@/components/ui/badge'
import { CardDescription, CardTitle } from '@/components/ui/card'

export function PipelineFormSectionHeader({
  title,
  summary,
  open,
  errorCount,
}: {
  title: string
  summary?: string
  open: boolean
  errorCount?: number
}) {
  return (
    <div className="flex w-full items-center justify-between">
      <div className="flex items-center gap-2">
        <CardTitle className="text-sm font-medium uppercase tracking-wider text-muted-foreground">
          {title}
        </CardTitle>
        {errorCount && errorCount > 0 ? (
          <Badge variant="destructive" className="text-[10px]">
            {errorCount} {errorCount === 1 ? 'error' : 'errors'}
          </Badge>
        ) : null}
      </div>
      <div className="flex items-center gap-2">
        {!open && summary ? (
          <CardDescription className="text-xs">{summary}</CardDescription>
        ) : null}
        <HugeiconsIcon
          icon={open ? ArrowUp01Icon : ArrowDown01Icon}
          size={16}
          className="text-muted-foreground"
        />
      </div>
    </div>
  )
}
