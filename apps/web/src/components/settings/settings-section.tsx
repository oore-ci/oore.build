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

/**
 * The longest description a section or subsection heading may carry. One
 * sentence of about 90 characters is the target (DESIGN.md, "Headings and
 * descriptions"); anything past this is documentation and belongs in a field
 * description, an empty state, a footnote, or the docs site.
 */
export const MAX_DESCRIPTION_LENGTH = 120

const warnedDescriptions = new Set<string>()

/** Dev-only nudge when a heading description outgrows one sentence. */
export function warnLongDescription(component: string, description?: string) {
  if (!import.meta.env.DEV || !description) return
  if (description.length <= MAX_DESCRIPTION_LENGTH) return
  if (warnedDescriptions.has(description)) return
  warnedDescriptions.add(description)
  console.warn(
    `${component}: description is ${description.length} characters, over the ${MAX_DESCRIPTION_LENGTH} limit. ` +
      'Keep it to one sentence or move it to a field, empty state, or footnote. ' +
      `"${description.slice(0, 60)}…"`,
  )
}

/** One-line muted copy under a section or subsection heading. */
export function SectionDescription({
  className,
  ...props
}: React.ComponentProps<'p'>) {
  return (
    <p
      data-slot="section-description"
      className={cn(
        'max-w-[65ch] text-[13px] leading-snug text-muted-foreground',
        className,
      )}
      {...props}
    />
  )
}

export function SettingsSection({
  actions,
  children,
  className,
  description,
  id,
  meta,
  title,
}: {
  actions?: React.ReactNode
  children?: React.ReactNode
  className?: string
  /** One sentence stating a rule that governs every control in the section. */
  description?: string
  /** Anchor for deep links; the heading clears sticky page chrome. */
  id?: string
  /** Status beside the title, such as a mode or count badge. Not prose. */
  meta?: React.ReactNode
  title: string
}) {
  const titleId = useId()
  warnLongDescription('SettingsSection', description)

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
          <div className="flex min-w-0 flex-wrap items-center gap-2">
            <h2 id={titleId} className="text-sm font-semibold">
              {title}
            </h2>
            {meta ?? null}
          </div>
          {description ? (
            <SectionDescription>{description}</SectionDescription>
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
