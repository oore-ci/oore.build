import { useId, type ReactNode } from 'react'

import {
  SectionHeading,
  SettingsSection,
  SettingsSurface,
  warnLongHint,
} from '@/components/settings/settings-section'
import { Badge } from '@/components/ui/badge'
import { Row } from '@/components/layout/row'
import { Stack } from '@/components/layout/stack'

/**
 * One top-level block of the pipeline editor: a heading outside one bordered
 * surface, following the settings anatomy in DESIGN.md. Sections are always
 * open; the editor has five of them (Start, Basics, Build, Signing,
 * Triggers) instead of nine equal collapsibles.
 */
export function PipelineFormSection({
  actions,
  children,
  errorCount,
  hint,
  id,
  title,
}: {
  actions?: ReactNode
  children: ReactNode
  errorCount?: number
  /** One sentence of help, shown in a tooltip behind an info icon. */
  hint?: string
  /** Anchor used by the section index and by deep links into the editor. */
  id?: string
  title: string
}) {
  const errorBadge =
    errorCount && errorCount > 0 ? (
      <Badge variant="destructive">
        {errorCount} {errorCount === 1 ? 'error' : 'errors'}
      </Badge>
    ) : null
  return (
    <SettingsSection
      id={id}
      hint={hint}
      title={title}
      actions={
        errorBadge || actions ? (
          <Row gap={2}>
            {errorBadge}
            {actions}
          </Row>
        ) : undefined
      }
    >
      <SettingsSurface className="flex flex-col gap-5">
        {children}
      </SettingsSurface>
    </SettingsSection>
  )
}

/**
 * A titled group inside a section surface. Divide consecutive subsections
 * with `SettingsSurfaceSeparator`.
 */
export function PipelineFormSubsection({
  actions,
  children,
  hint,
  id,
  meta,
  title,
}: {
  actions?: ReactNode
  children: ReactNode
  /** One sentence of help, shown in a tooltip behind an info icon. */
  hint?: string
  id?: string
  /** Current state beside the title, such as a signing summary. Not prose. */
  meta?: ReactNode
  title: string
}) {
  const headingId = useId()
  warnLongHint('PipelineFormSubsection', hint)
  return (
    <Stack
      id={id}
      role="group"
      aria-labelledby={headingId}
      gap={4}
      className="scroll-mt-[calc(var(--header-height)+4rem)]"
    >
      <div className="flex items-start justify-between gap-3">
        <SectionHeading
          as="h3"
          hint={hint}
          id={headingId}
          meta={meta}
          title={title}
        />
        {actions ? <div className="shrink-0">{actions}</div> : null}
      </div>
      {children}
    </Stack>
  )
}
