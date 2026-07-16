import * as React from 'react'
import { UnfoldMoreIcon } from '@hugeicons/core-free-icons'
import { HugeiconsIcon } from '@hugeicons/react'

import { cn } from '@/lib/utils'

function NativeSelect({
  className,
  ...props
}: React.ComponentProps<'select'>) {
  return (
    <div
      className={cn('relative w-fit', className)}
      data-slot="native-select-wrapper"
    >
      <select
        data-slot="native-select"
        className="h-9 w-full appearance-none border border-input bg-background px-3 pr-8 text-sm outline-none transition-colors focus-visible:border-ring focus-visible:ring-2 focus-visible:ring-ring/50 disabled:pointer-events-none disabled:opacity-50"
        {...props}
      />
      <HugeiconsIcon
        icon={UnfoldMoreIcon}
        className="pointer-events-none absolute top-1/2 right-2.5 size-4 -translate-y-1/2 text-muted-foreground select-none"
        aria-hidden
        data-slot="native-select-icon"
      />
    </div>
  )
}

function NativeSelectOption({
  className,
  ...props
}: React.ComponentProps<'option'>) {
  return (
    <option
      data-slot="native-select-option"
      className={className}
      {...props}
    />
  )
}

export { NativeSelect, NativeSelectOption }
