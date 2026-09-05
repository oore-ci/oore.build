import { lazy, Suspense, useState } from 'react'
import PageBreadcrumb from './page-breadcrumb'
import { Button } from './ui/button'
import { Kbd } from './ui/kbd'
import { Separator } from './ui/separator'
import { SidebarTrigger } from './ui/sidebar'
import { HugeiconsIcon } from '@hugeicons/react'
import { SearchIcon } from '@hugeicons/core-free-icons'
import { useHotkey } from '@tanstack/react-hotkeys'
import { OperatorIncidentNotifications } from './operator-incident-notifications'

const CommandPalette = lazy(() => import('./command-palette'))

export default function SiteHeader() {
  const [commandPaletteOpen, setCommandPaletteOpen] = useState(false)

  useHotkey('Mod+K', () => {
    setCommandPaletteOpen((open) => !open)
  })

  const openCommandPalette = () => setCommandPaletteOpen(true)

  return (
    <header className="sticky top-0 z-30 flex h-(--header-height) shrink-0 items-center gap-2 border-b bg-background/90 backdrop-blur-md transition-[width,height] ease-linear group-has-data-[collapsible=icon]/sidebar-wrapper:h-(--header-height) supports-[backdrop-filter]:bg-background/75 md:rounded-t-xl">
      <div className="flex w-full items-center gap-1 px-4 lg:gap-2 lg:px-6">
        <SidebarTrigger className="-ml-1" />
        <Separator orientation="vertical" className="mx-2 h-4 self-center!" />
        <PageBreadcrumb />
        <div className="ml-auto flex items-center gap-2">
          <OperatorIncidentNotifications />
          <Button
            variant="outline"
            size="default"
            className="flex"
            aria-label="Quick open"
            aria-haspopup="dialog"
            aria-expanded={commandPaletteOpen}
            onClick={openCommandPalette}
          >
            <HugeiconsIcon icon={SearchIcon} data-icon="inline-start" />
            <span className="hidden sm:inline">Quick open</span>
            <Kbd className="hidden sm:inline-flex">⌘K</Kbd>
          </Button>
        </div>
        {commandPaletteOpen ? (
          <Suspense fallback={null}>
            <CommandPalette open onOpenChange={setCommandPaletteOpen} />
          </Suspense>
        ) : null}
      </div>
    </header>
  )
}
