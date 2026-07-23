import {
  ArrowDown as ArrowDown01Icon,
  ArrowUp as ArrowUp01Icon,
} from 'lucide-react'

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
  const Icon = open ? ArrowUp01Icon : ArrowDown01Icon

  return (
    <div className="flex w-full items-center justify-between">
      <div className="flex items-center gap-2">
        <CardTitle className="text-sm font-medium text-muted-foreground">
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
        <Icon size={16} className="text-muted-foreground" />
      </div>
    </div>
  )
}
