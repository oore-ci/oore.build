import { useState } from 'react'
import type { ReactNode } from 'react'
import { DynamicLucideIcon } from '@/components/ui/dynamic-lucide-icon'
import {
  ArrowDown as ArrowDown01Icon,
  ArrowRight as ArrowRight01Icon,
} from 'lucide-react'

import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible'

export function PipelineDetailSection({
  title,
  defaultOpen = false,
  children,
}: {
  title: string
  defaultOpen?: boolean
  children: ReactNode
}) {
  const [open, setOpen] = useState(defaultOpen)

  return (
    <Collapsible open={open} onOpenChange={setOpen}>
      <CollapsibleTrigger className="flex w-full items-center gap-2 py-2 text-sm font-medium">
        <DynamicLucideIcon
          icon={open ? ArrowDown01Icon : ArrowRight01Icon}
          size={14}
        />
        {title}
      </CollapsibleTrigger>
      <CollapsibleContent>
        <div className="pb-2">{children}</div>
      </CollapsibleContent>
    </Collapsible>
  )
}

export function PipelineDetailValue({
  label,
  children,
}: {
  label: string
  children: ReactNode
}) {
  return (
    <div className="flex gap-4 py-1 text-xs">
      <span className="w-40 shrink-0 text-muted-foreground">{label}</span>
      <span className="min-w-0 break-all">{children}</span>
    </div>
  )
}
