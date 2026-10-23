import { lazy, Suspense, useState } from 'react'
import PageBreadcrumb from './page-breadcrumb'
import { Button } from './ui/button'
import { Kbd } from './ui/kbd'
import { SidebarTrigger } from './ui/sidebar'
import { HugeiconsIcon } from '@hugeicons/react'
import { SearchIcon } from '@hugeicons/core-free-icons'
import { detectPlatform, useHotkey } from '@tanstack/react-hotkeys'
import { useNavigate } from '@tanstack/react-router'
import { OperatorIncidentNotifications } from './operator-incident-notifications'
import { GlobalShortcuts } from './global-shortcuts'
import {
  SHORTCUTS,
  registerableHotkey,
  shortcutHint,
  shortcutOptions,
} from '@/lib/keyboard-shortcuts'
import { useCommandStore } from '@/stores/command-store'
import type { RunBuildTarget } from '@/stores/command-store'

const CommandPalette = lazy(() => import('./command-palette'))
const TriggerBuildDrawer = lazy(() => import('./trigger-build-drawer'))

export default function SiteHeader() {
  const navigate = useNavigate()
  const commandPaletteOpen = useCommandStore((s) => s.paletteOpen)
  const setCommandPaletteOpen = useCommandStore((s) => s.setPaletteOpen)
  const [runBuildOpen, setRunBuildOpen] = useState(false)
  // Stays mounted after the first open so closing can animate.
  const [runBuildMounted, setRunBuildMounted] = useState(false)
  const [runBuildTarget, setRunBuildTarget] = useState<RunBuildTarget | null>(
    null,
  )

  useHotkey(
    registerableHotkey(SHORTCUTS.palette.hotkey),
    () => {
      setCommandPaletteOpen(!useCommandStore.getState().paletteOpen)
    },
    { meta: shortcutOptions(SHORTCUTS.palette).meta },
  )

  const openCommandPalette = () => setCommandPaletteOpen(true)

  return (
    <header className="sticky top-0 z-30 flex h-(--header-height) shrink-0 items-center gap-2 border-b bg-background/90 backdrop-blur-md transition-[width,height] ease-linear supports-[backdrop-filter]:bg-background/75 md:rounded-t-xl">
      <GlobalShortcuts />
      <div className="flex w-full items-center gap-1 px-4 lg:gap-2 lg:px-6">
        <SidebarTrigger className="-ml-1" />
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
            <Kbd className="hidden sm:inline-flex">
              {shortcutHint(SHORTCUTS.palette, detectPlatform())}
            </Kbd>
          </Button>
        </div>
        {commandPaletteOpen ? (
          <Suspense fallback={null}>
            <CommandPalette
              open
              onOpenChange={setCommandPaletteOpen}
              onRunBuild={(target) => {
                setRunBuildTarget(target ?? null)
                setRunBuildMounted(true)
                setRunBuildOpen(true)
              }}
            />
          </Suspense>
        ) : null}
        {runBuildMounted ? (
          <Suspense fallback={null}>
            <TriggerBuildDrawer
              key={runBuildTarget?.pipelineId ?? 'any'}
              fixedProjectId={runBuildTarget?.projectId}
              fixedPipelineId={runBuildTarget?.pipelineId}
              fixedPipelineName={runBuildTarget?.pipelineName}
              description={
                runBuildTarget
                  ? 'Choose a branch to build.'
                  : 'Choose a project, pipeline and branch to build.'
              }
              open={runBuildOpen}
              onOpenChange={setRunBuildOpen}
              onBuildCreated={(buildId) => {
                void navigate({ to: '/builds/$buildId', params: { buildId } })
              }}
            />
          </Suspense>
        ) : null}
      </div>
    </header>
  )
}
