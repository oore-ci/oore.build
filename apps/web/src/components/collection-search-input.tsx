import { useState } from 'react'
import { Search01Icon } from '@hugeicons/core-free-icons'
import { HugeiconsIcon } from '@hugeicons/react'

import { useDebouncedCallback } from '@/hooks/use-debounced-callback'
import { Input } from '@/components/ui/input'
import { cn } from '@/lib/utils'

export function CollectionSearchInput({
  ariaLabel,
  className,
  initialValue,
  onSearch,
  placeholder,
}: {
  ariaLabel: string
  className?: string
  initialValue: string
  onSearch: (value: string) => void
  placeholder: string
}) {
  const [value, setValue] = useState(initialValue)
  const debouncedSearch = useDebouncedCallback(onSearch, 300)

  return (
    <div className={cn('relative w-full sm:max-w-sm', className)}>
      <HugeiconsIcon
        icon={Search01Icon}
        className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground"
        aria-hidden
      />
      <Input
        type="search"
        value={value}
        onChange={(event) => {
          const nextValue = event.target.value
          setValue(nextValue)
          debouncedSearch(nextValue)
        }}
        placeholder={placeholder}
        aria-label={ariaLabel}
        className="pl-9"
      />
    </div>
  )
}
