import { HugeiconsIcon } from '@hugeicons/react'
import { InformationCircleIcon } from '@hugeicons/core-free-icons'

import { Button } from '@/components/ui/button'
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from '@/components/ui/tooltip'

/**
 * One sentence of help behind an info icon beside a heading or a field
 * label. It opens on hover and on keyboard focus, so the sentence is there
 * when someone wants it and the screen stays quiet otherwise.
 */
export function InfoHint({
  children,
  label,
}: {
  children: React.ReactNode
  /** What the hint belongs to, for the icon's accessible name. */
  label?: string
}) {
  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <Button
            type="button"
            variant="ghost"
            size="icon-xs"
            className="-my-1 text-muted-foreground"
          />
        }
      >
        <HugeiconsIcon icon={InformationCircleIcon} />
        <span className="sr-only">
          {label ? `About ${label}` : 'More information'}
        </span>
      </TooltipTrigger>
      <TooltipContent side="top" align="start" className="text-pretty">
        {children}
      </TooltipContent>
    </Tooltip>
  )
}
