import * as React from 'react'
import { UnfoldMoreIcon } from '@hugeicons/core-free-icons'
import { HugeiconsIcon } from '@hugeicons/react'

import { cn } from '@/lib/utils'

function NativeSelect({ className, ...props }: React.ComponentProps<'select'>) {
  return (
    <div
      className={cn(
        'cn-native-select-wrapper group/native-select relative w-fit has-[select:disabled]:opacity-50',
        className,
      )}
      data-slot="native-select-wrapper"
    >
      <select
        data-slot="native-select"
        className="cn-native-select outline-none disabled:pointer-events-none disabled:cursor-not-allowed"
        {...props}
      />
      <HugeiconsIcon
        icon={UnfoldMoreIcon}
        className="cn-native-select-icon pointer-events-none absolute select-none"
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
      className={cn('bg-[Canvas] text-[CanvasText]', className)}
      {...props}
    />
  )
}

export { NativeSelect, NativeSelectOption }
