import { useEffect, useRef } from 'react'
import type { IconSvgElement } from '@hugeicons/react'
import { create } from 'zustand'

/** An action the current page offers in Quick open, such as Re-run. */
export interface PageCommand {
  id: string
  label: string
  icon: IconSvgElement
  /** Shown beside the row, e.g. `r`. */
  hint?: string
  keywords?: Array<string>
  run: () => void
}

/** A pipeline Quick open should preselect in the Run build drawer. */
export interface RunBuildTarget {
  projectId: string
  pipelineId: string
  pipelineName: string
}

interface CommandState {
  paletteOpen: boolean
  setPaletteOpen: (open: boolean) => void
  keyboardMapOpen: boolean
  setKeyboardMapOpen: (open: boolean) => void
  pageCommandsHeading: string | null
  pageCommands: Array<PageCommand>
  setPageCommands: (
    heading: string | null,
    commands: Array<PageCommand>,
  ) => void
}

export const useCommandStore = create<CommandState>((set) => ({
  paletteOpen: false,
  setPaletteOpen: (paletteOpen) => set({ paletteOpen }),
  keyboardMapOpen: false,
  setKeyboardMapOpen: (keyboardMapOpen) => set({ keyboardMapOpen }),
  pageCommandsHeading: null,
  pageCommands: [],
  setPageCommands: (pageCommandsHeading, pageCommands) =>
    set({ pageCommandsHeading, pageCommands }),
}))

/**
 * Offers the page's actions in Quick open while the page is mounted. The
 * store only changes when the set of actions does; each `run` calls the
 * latest callback, so it never sees stale page state.
 */
export function usePageCommands(heading: string, commands: Array<PageCommand>) {
  const latest = useRef(commands)
  useEffect(() => {
    latest.current = commands
  })
  const signature = [
    heading,
    ...commands.map((command) => `${command.id}\u0000${command.label}`),
  ].join('\u0001')

  useEffect(() => {
    const current = latest.current.map((command) => ({
      ...command,
      run: () => latest.current.find((item) => item.id === command.id)?.run(),
    }))
    useCommandStore.getState().setPageCommands(heading, current)
    return () => useCommandStore.getState().setPageCommands(null, [])
  }, [heading, signature])
}
