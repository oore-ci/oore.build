import { useId } from 'react'

import { cn } from '@/lib/utils'
import { Separator } from '@/components/ui/separator'

export function SettingsSurface({
  className,
  inset = true,
  ...props
}: React.ComponentProps<'div'> & { inset?: boolean }) {
  return (
    <div
      data-slot="settings-surface"
      // `overflow-clip`, not `hidden`: a hidden-overflow box is a scroll
      // container, so anchor links inside it would scroll the box instead
      // of the page.
      className={cn(
        'overflow-clip rounded-lg border bg-card text-card-foreground',
        inset && 'p-4 sm:p-5',
        className,
      )}
      {...props}
    />
  )
}

/** Divides subsections edge to edge inside an inset `SettingsSurface`. */
export function SettingsSurfaceSeparator() {
  return <Separator className="-mx-4 sm:-mx-5 data-horizontal:w-auto" />
}

export function SettingsSection({
  actions,
  children,
  className,
  description,
  id,
  title,
}: {
  actions?: React.ReactNode
  children?: React.ReactNode
  className?: string
  description?: string
  /** Anchor for deep links; the heading clears sticky page chrome. */
  id?: string
  title: string
}) {
  const titleId = useId()

  return (
    <section
      id={id}
      aria-labelledby={titleId}
      className={cn(
        'flex min-w-0 scroll-mt-[calc(var(--header-height)+4rem)] flex-col gap-4',
        className,
      )}
    >
      <div className="flex items-start justify-between gap-4">
        <div className="flex min-w-0 flex-col gap-1">
          <h2 id={titleId} className="text-sm font-semibold">
            {title}
          </h2>
          {description ? (
            <p className="max-w-[65ch] text-sm text-muted-foreground">
              {description}
            </p>
          ) : null}
        </div>
        {actions ? (
          <div className="flex shrink-0 flex-wrap items-center justify-end gap-2">
            {actions}
          </div>
        ) : null}
      </div>
      {children ?? null}
    </section>
  )
}
