import { useHotkey, useHotkeySequence } from '@tanstack/react-hotkeys'

import { registerableHotkey, shortcutOptions } from '@/lib/keyboard-shortcuts'
import type {
  KeyShortcutDefinition,
  SequenceShortcutDefinition,
} from '@/lib/keyboard-shortcuts'

const OVERLAY_SELECTOR = [
  'dialog-content',
  'alert-dialog-content',
  'drawer-content',
  'sheet-content',
  'dropdown-menu-content',
  'popover-content',
  'select-content',
  'combobox-content',
]
  .map((slot) => `[data-slot="${slot}"]`)
  .join(',')

/** True while a dialog, drawer or menu owns the keyboard. */
export function isOverlayOpen() {
  return document.querySelector(OVERLAY_SELECTOR) !== null
}

/**
 * Registers one of the app's single-key shortcuts. It never fires inside
 * text fields or editors, nor while a dialog or menu is open, and it is
 * listed in the keyboard map only while `enabled`. Return `false` from the
 * callback to leave the key to the browser.
 */
export function useAppShortcut(
  shortcut: KeyShortcutDefinition,
  callback: () => boolean | void,
  enabled = true,
) {
  useHotkey(
    registerableHotkey(shortcut.hotkey),
    (event) => {
      if (isOverlayOpen()) return
      if (callback() !== false) event.preventDefault()
    },
    {
      ...shortcutOptions(shortcut),
      enabled,
      // Decided in the callback, so a key that does nothing here keeps its
      // normal browser behavior (Enter still presses a focused button).
      preventDefault: false,
      stopPropagation: false,
    },
  )
}

/** Registers one of the app's `g` jump sequences. */
export function useAppSequence(
  shortcut: SequenceShortcutDefinition,
  callback: () => void,
  enabled = true,
) {
  useHotkeySequence(
    shortcut.sequence,
    () => {
      if (isOverlayOpen()) return
      callback()
    },
    { ...shortcutOptions(shortcut), enabled },
  )
}
