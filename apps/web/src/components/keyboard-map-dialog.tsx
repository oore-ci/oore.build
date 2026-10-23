import { Fragment, useState } from 'react'
import {
  detectPlatform,
  getHotkeyManager,
  getSequenceManager,
} from '@tanstack/react-hotkeys'

import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Kbd, KbdGroup } from '@/components/ui/kbd'
import {
  groupShortcutEntries,
  shortcutKeyCaps,
  type ShortcutEntry,
} from '@/lib/keyboard-shortcuts'

/** The sidebar's own shortcut, which comes with the shadcn primitive. */
const SIDEBAR_ENTRY: ShortcutEntry = {
  id: 'sidebar',
  label: 'Toggle sidebar',
  steps: ['Mod+B'],
}

function readSections() {
  const hotkeys = [...getHotkeyManager().registrations.state.values()]
  const sequences = [...getSequenceManager().registrations.state.values()]
  const entries: Array<ShortcutEntry> = [
    ...hotkeys.flatMap((registration) =>
      registration.options.enabled !== false && registration.options.meta?.name
        ? [
            {
              id: registration.id,
              label: registration.options.meta.name,
              steps: [registration.hotkey],
            },
          ]
        : [],
    ),
    SIDEBAR_ENTRY,
    ...sequences.flatMap((registration) =>
      registration.options.enabled !== false && registration.options.meta?.name
        ? [
            {
              id: registration.id,
              label: registration.options.meta.name,
              steps: [...registration.sequence],
            },
          ]
        : [],
    ),
  ]
  return groupShortcutEntries(entries)
}

export default function KeyboardMapDialog({
  open,
  onOpenChange,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  const platform = detectPlatform()
  // Read once when the map opens. Subscribing would re-render the map while
  // other components register hotkeys during their own render.
  const [sections] = useState(readSections)

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader className="pr-10">
          <DialogTitle>Keyboard shortcuts</DialogTitle>
          <DialogDescription>
            Shortcuts for this page. They never fire while you type.
          </DialogDescription>
        </DialogHeader>
        <div className="flex flex-col gap-5">
          {sections.map((section) => (
            <section
              key={section.group}
              aria-labelledby={`shortcuts-${section.group.replaceAll(' ', '-')}`}
            >
              <h3
                id={`shortcuts-${section.group.replaceAll(' ', '-')}`}
                className="mb-1.5 text-xs font-medium text-muted-foreground"
              >
                {section.group}
              </h3>
              <dl className="divide-y rounded-xl border">
                {section.entries.map((entry) => (
                  <div
                    key={entry.id}
                    className="flex items-center justify-between gap-4 px-3 py-2"
                  >
                    <dt className="min-w-0 truncate">{entry.label}</dt>
                    <dd className="flex shrink-0 items-center gap-1.5 text-xs text-muted-foreground">
                      {entry.steps.map((step, index) => (
                        <Fragment key={`${step}-${index}`}>
                          {index > 0 ? <span>then</span> : null}
                          <KbdGroup>
                            {shortcutKeyCaps(step, platform).map((cap) => (
                              <Kbd key={cap} className="min-w-6 font-mono">
                                {cap}
                              </Kbd>
                            ))}
                          </KbdGroup>
                        </Fragment>
                      ))}
                    </dd>
                  </div>
                ))}
              </dl>
            </section>
          ))}
        </div>
      </DialogContent>
    </Dialog>
  )
}
