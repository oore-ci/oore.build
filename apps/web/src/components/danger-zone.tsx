import type { ReactNode } from 'react'
import { useId } from 'react'

import { SettingsSurface } from '@/components/settings/settings-section'
import { Text } from '@/components/layout/text'
import { splitRowVariants } from '@/lib/recipes'
import { cn } from '@/lib/utils'

/**
 * The final section of a detail or settings page that holds its destructive
 * action. Headers never carry Delete; it lives here, and the action itself
 * confirms through an `AlertDialog`.
 */
export function DangerZone({
  action,
  description,
  title,
}: {
  action: ReactNode
  description: string
  title: string
}) {
  const headingId = useId()
  return (
    <section aria-labelledby={headingId} className="flex flex-col gap-3">
      <h2 id={headingId} className="text-sm font-semibold text-destructive">
        Danger zone
      </h2>
      <SettingsSurface
        className={cn(splitRowVariants(), 'border-destructive/40')}
      >
        <div className="min-w-0 space-y-1">
          <p className="text-sm font-medium">{title}</p>
          <Text variant="muted" className="max-w-[65ch]">
            {description}
          </Text>
        </div>
        <div className="flex shrink-0 justify-end">{action}</div>
      </SettingsSurface>
    </section>
  )
}
