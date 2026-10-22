import { useEffect, useState } from 'react'

import { cn } from '@/lib/utils'

/**
 * Jump links for the long pipeline form. Sticks to the top of the scroll
 * area and marks the section currently under it.
 */
export function PipelineSectionIndex({
  sections,
}: {
  sections: Array<{ id: string; label: string }>
}) {
  const [activeId, setActiveId] = useState(sections[0]?.id)
  const sectionKey = sections.map((section) => section.id).join(',')

  useEffect(() => {
    const ids = sectionKey.split(',')
    const elements = ids
      .map((id) => document.getElementById(id))
      .filter((element): element is HTMLElement => element !== null)
    if (elements.length === 0) return
    const visible = new Map<string, boolean>()
    const observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          visible.set(entry.target.id, entry.isIntersecting)
        }
        const first = ids.find((id) => visible.get(id))
        if (first) setActiveId(first)
      },
      // A section counts once its top passes under the index and stays
      // active until the next one does.
      { rootMargin: '-120px 0px -55% 0px' },
    )
    for (const element of elements) observer.observe(element)
    return () => observer.disconnect()
  }, [sectionKey])

  return (
    <nav
      aria-label="Pipeline sections"
      className="sticky top-(--header-height) z-20 -mx-1 -mb-4 flex gap-1 overflow-x-auto bg-background/95 px-1 py-2 backdrop-blur supports-backdrop-filter:bg-background/80"
    >
      {sections.map((section) => (
        <a
          key={section.id}
          data-slot="button"
          href={`#${section.id}`}
          aria-current={activeId === section.id ? 'location' : undefined}
          onClick={(event) => {
            event.preventDefault()
            document
              .getElementById(section.id)
              ?.scrollIntoView({ block: 'start' })
            window.history.replaceState(
              window.history.state,
              '',
              `#${section.id}`,
            )
            setActiveId(section.id)
          }}
          className={cn(
            'inline-flex h-8 shrink-0 items-center rounded-2xl px-3 text-sm font-medium text-muted-foreground transition-colors outline-none hover:bg-muted hover:text-foreground focus-visible:ring-3 focus-visible:ring-ring/30',
            activeId === section.id && 'bg-muted text-foreground',
          )}
        >
          {section.label}
        </a>
      ))}
    </nav>
  )
}
