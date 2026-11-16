import {
  createContext,
  useContext,
  useEffect,
  useId,
  useRef,
  useState,
  type ReactNode,
} from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import { ArrowDown01Icon } from '@hugeicons/core-free-icons'

import { cn } from '@/lib/utils'
import {
  SectionHeading,
  SettingsSection,
  SettingsSurface,
  warnLongHint,
} from '@/components/settings/settings-section'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible'
import { Row } from '@/components/layout/row'
import { Stack } from '@/components/layout/stack'

const OPEN_EVENT = 'oore:pipeline-section-open'
const REVEAL_INVALID_EVENT = 'oore:pipeline-section-reveal-invalid'

/** Opens the section that holds the element with this id. */
export function openPipelineSection(id: string) {
  window.dispatchEvent(new CustomEvent(OPEN_EVENT, { detail: id }))
}

/** Opens every section that holds an invalid field, after a failed save. */
export function revealInvalidPipelineSections() {
  window.dispatchEvent(new Event(REVEAL_INVALID_EVENT))
}

/** Whether sections start closed with a summary or open. */
export const PipelineSectionsCollapsible = createContext(false)

/**
 * The title of the pipeline page row a section renders in. Inside a row a
 * section is a plain group: no surface of its own, and no heading when it
 * repeats the row's title.
 */
export const PipelineSectionsEmbeddedIn = createContext<string | null>(null)

/**
 * Inside a row whose other values come from the pipeline's file, groups
 * of fields say they are set here.
 */
export const PipelineSectionsSetHere = createContext(false)

/**
 * One top-level block of the pipeline editor: a heading outside one bordered
 * surface, following the settings anatomy in DESIGN.md. On the Edit page a
 * section with a `summary` starts closed and shows that line instead; a
 * link to its anchor (or to an anchor inside it) and a failed save open it.
 */
export function PipelineFormSection({
  actions,
  children,
  errorCount,
  hint,
  id,
  summary,
  title,
}: {
  actions?: ReactNode
  children: ReactNode
  errorCount?: number
  /** One sentence of help, shown in a tooltip behind an info icon. */
  hint?: string
  /** Anchor used by the section index and by deep links into the editor. */
  id?: string
  /** What is set inside, shown while the section is closed. */
  summary?: string
  title: string
}) {
  const embeddedIn = useContext(PipelineSectionsEmbeddedIn)
  const markSetHere = useContext(PipelineSectionsSetHere)
  const collapsible =
    useContext(PipelineSectionsCollapsible) && !!summary && embeddedIn === null
  const [open, setOpen] = useState(!collapsible)
  const ref = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (!collapsible) return
    function openFor(target: string) {
      const root = ref.current
      if (!root) return
      const element = document.getElementById(target)
      if (element && root.contains(element)) setOpen(true)
    }
    const hash = window.location.hash.slice(1)
    if (hash) openFor(hash)
    function onOpen(event: Event) {
      if (event instanceof CustomEvent) openFor(String(event.detail))
    }
    function onRevealInvalid() {
      if (ref.current?.querySelector('[aria-invalid="true"]')) setOpen(true)
    }
    window.addEventListener(OPEN_EVENT, onOpen)
    window.addEventListener(REVEAL_INVALID_EVENT, onRevealInvalid)
    return () => {
      window.removeEventListener(OPEN_EVENT, onOpen)
      window.removeEventListener(REVEAL_INVALID_EVENT, onRevealInvalid)
    }
  }, [collapsible])

  if (embeddedIn !== null) {
    if (title === embeddedIn) {
      return (
        <Stack
          id={id}
          gap={5}
          className="scroll-mt-[calc(var(--header-height)+4rem)]"
        >
          {children}
          {actions ? <Row className="justify-end">{actions}</Row> : null}
        </Stack>
      )
    }
    return (
      <PipelineFormSubsection
        actions={actions}
        hint={hint}
        id={id}
        meta={
          markSetHere ? (
            <span className="text-[13px] text-muted-foreground">set here</span>
          ) : undefined
        }
        title={title}
      >
        {children}
      </PipelineFormSubsection>
    )
  }

  const errorBadge =
    errorCount && errorCount > 0 ? (
      <Badge variant="destructive">
        {errorCount} {errorCount === 1 ? 'error' : 'errors'}
      </Badge>
    ) : null
  const toggle = collapsible ? (
    <CollapsibleTrigger
      render={
        <Button
          type="button"
          variant="ghost"
          size="sm"
          className="text-muted-foreground"
        />
      }
    >
      {open ? 'Close' : 'Open'}
      <HugeiconsIcon
        icon={ArrowDown01Icon}
        data-icon="inline-end"
        className={cn('transition-transform', open && 'rotate-180')}
      />
    </CollapsibleTrigger>
  ) : null
  const surface = (
    <SettingsSurface className="flex flex-col gap-5">
      {children}
    </SettingsSurface>
  )
  const section = (
    <SettingsSection
      id={id}
      hint={hint}
      title={title}
      status={collapsible && !open ? summary : undefined}
      actions={
        errorBadge || actions || toggle ? (
          <Row gap={2}>
            {errorBadge}
            {open ? actions : null}
            {toggle}
          </Row>
        ) : undefined
      }
    >
      {collapsible ? (
        <CollapsibleContent keepMounted>{surface}</CollapsibleContent>
      ) : (
        surface
      )}
    </SettingsSection>
  )
  if (!collapsible) return section
  return (
    <Collapsible ref={ref} open={open} onOpenChange={setOpen}>
      {section}
    </Collapsible>
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
