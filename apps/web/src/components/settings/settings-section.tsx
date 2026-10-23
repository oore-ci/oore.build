import { useId } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import { InformationCircleIcon } from '@hugeicons/core-free-icons'

import { cn } from '@/lib/utils'
import { Button } from '@/components/ui/button'
import { Separator } from '@/components/ui/separator'
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from '@/components/ui/tooltip'

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
 * The longest hint a section or subsection heading may carry. One sentence
 * of about 90 characters is the target (DESIGN.md, "Headings and hints");
 * anything past this is documentation and belongs in a field description,
 * an empty state, a footnote, or the docs site.
 */
export const MAX_HINT_LENGTH = 120

const warnedHints = new Set<string>()

/** Dev-only nudge when a heading hint outgrows one sentence. */
export function warnLongHint(component: string, hint?: string) {
  if (!import.meta.env.DEV || !hint) return
  if (hint.length <= MAX_HINT_LENGTH) return
  if (warnedHints.has(hint)) return
  warnedHints.add(hint)
  console.warn(
    `${component}: hint is ${hint.length} characters, over the ${MAX_HINT_LENGTH} limit. ` +
      'Keep it to one sentence or move it to a field, empty state, or footnote. ' +
      `"${hint.slice(0, 60)}…"`,
  )
}

/**
 * One sentence of help behind an info icon beside a heading. It opens on
 * hover and on keyboard focus, so the sentence is there when someone wants
 * it and the heading row stays one line otherwise.
 */
export function SectionHint({
  children,
  label,
}: {
  children: string
  /** The heading the hint belongs to, for the icon's accessible name. */
  label: string
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
        <span className="sr-only">About {label}</span>
      </TooltipTrigger>
      <TooltipContent side="top" align="start" className="text-pretty">
        {children}
      </TooltipContent>
    </Tooltip>
  )
}

/** The heading row shared by sections and subsections. */
export function SectionHeading({
  as: Heading,
  hint,
  id,
  meta,
  status,
  title,
}: {
  as: 'h2' | 'h3'
  hint?: string
  id: string
  meta?: React.ReactNode
  status?: React.ReactNode
  title: string
}) {
  return (
    <div className="flex min-w-0 flex-col gap-1">
      <div className="flex min-w-0 flex-wrap items-center gap-x-1.5 gap-y-1">
        <Heading
          id={id}
          className={cn(
            Heading === 'h2' ? 'font-semibold' : 'font-medium',
            'text-sm',
          )}
        >
          {title}
        </Heading>
        {hint ? <SectionHint label={title}>{hint}</SectionHint> : null}
        {meta ? (
          <span className="ms-0.5 inline-flex items-center gap-2">{meta}</span>
        ) : null}
      </div>
      {status ? (
        <p className="max-w-[65ch] text-[13px] leading-snug text-muted-foreground">
          {status}
        </p>
      ) : null}
    </div>
  )
}

export function SettingsSection({
  actions,
  children,
  className,
  hint,
  id,
  meta,
  status,
  title,
}: {
  actions?: React.ReactNode
  children?: React.ReactNode
  className?: string
  /** One sentence of help, shown in a tooltip behind an info icon. */
  hint?: string
  /** Anchor for deep links; the heading clears sticky page chrome. */
  id?: string
  /** Status beside the title, such as a mode or count badge. Not prose. */
  meta?: React.ReactNode
  /**
   * One line of live state under the title, such as what a toggle will do
   * with the current form values. Never static copy; that is `hint`.
   */
  status?: React.ReactNode
  title: string
}) {
  const titleId = useId()
  warnLongHint('SettingsSection', hint)

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
        <SectionHeading
          as="h2"
          hint={hint}
          id={titleId}
          meta={meta}
          status={status}
          title={title}
        />
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
