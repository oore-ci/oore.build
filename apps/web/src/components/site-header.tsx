import PageBreadcrumb from './page-breadcrumb'
import { Button } from './ui/button'
import { Kbd } from './ui/kbd'
import { Separator } from './ui/separator'
import { SidebarTrigger } from './ui/sidebar'
import { SearchIcon } from 'lucide-react'
import { useHotkey } from '@tanstack/react-hotkeys'
import CommandPalette from './command-palette'
import { useUiStore } from '@/stores/ui-store'

export default function SiteHeader() {
  useHotkey('Mod+K', () => {
    const currentState = useUiStore.getState()
    currentState.setCommandPaletteOpen(!currentState.commandPaletteOpen)
  })

  return (
    <header className="flex h-(--header-height) shrink-0 items-center gap-2 border-b transition-[width,height] ease-linear group-has-data-[collapsible=icon]/sidebar-wrapper:h-(--header-height)">
      <div className="flex w-full items-center gap-1 px-4 lg:gap-2 lg:px-6">
        <SidebarTrigger className="-ml-1" />
        <Separator
          orientation="vertical"
          className="mx-2 data-[orientation=vertical]:h-4"
        />
        <PageBreadcrumb />
        <div className="ml-auto flex items-center gap-2">
          <Button variant="outline" size="default" className="hidden sm:flex">
            <SearchIcon />
            <span className="hidden sm:inline">Search</span>
            <Kbd className="hidden sm:inline-flex">⌘K</Kbd>
          </Button>
        </div>
        <CommandPalette />
      </div>
    </header>
  )
}
