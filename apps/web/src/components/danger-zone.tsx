import type { ReactNode } from 'react'
import { useId } from 'react'

import { SettingsSurface } from '@/components/settings/settings-section'

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
      <SettingsSurface className="flex flex-col gap-3 border-destructive/40 sm:flex-row sm:items-center sm:justify-between">
        <div className="min-w-0 space-y-1">
          <p className="text-sm font-medium">{title}</p>
          <p className="max-w-[65ch] text-sm text-muted-foreground">
            {description}
          </p>
        </div>
        <div className="flex shrink-0 justify-end">{action}</div>
      </SettingsSurface>
    </section>
  )
}
