import type {
  Hotkey,
  HotkeySequence,
  RegisterableHotkey,
} from '@tanstack/react-hotkeys'

export type ShortcutGroup = 'General' | 'Go to' | 'Lists' | 'This build'

const GROUP_ORDER: ReadonlyArray<ShortcutGroup> = [
  'General',
  'Go to',
  'Lists',
  'This build',
]

/** `?` is Shift+/ on the keyboards the map is written for. */
type QuestionMark = 'Shift+/'

export interface KeyShortcutDefinition {
  hotkey: Hotkey | QuestionMark
  label: string
  group: ShortcutGroup
}

export interface SequenceShortcutDefinition {
  sequence: HotkeySequence
  label: string
  group: ShortcutGroup
}

type ShortcutDefinition = KeyShortcutDefinition | SequenceShortcutDefinition

/**
 * Every app-wide binding in one place, so the keyboard map, the palette hints
 * and the registrations agree. Pages register only the ones they support.
 */
export const SHORTCUTS = {
  palette: { hotkey: 'Mod+K', label: 'Open Quick open', group: 'General' },
  keyboardMap: {
    hotkey: 'Shift+/',
    label: 'Show keyboard shortcuts',
    group: 'General',
  },
  search: { hotkey: '/', label: 'Search', group: 'General' },
  toggleTheme: { hotkey: 'D', label: 'Toggle dark mode', group: 'General' },
  goHome: { sequence: ['G', 'H'], label: 'Go to Home', group: 'Go to' },
  goBuilds: { sequence: ['G', 'B'], label: 'Go to Builds', group: 'Go to' },
  goProjects: {
    sequence: ['G', 'P'],
    label: 'Go to Projects',
    group: 'Go to',
  },
  goSettings: {
    sequence: ['G', 'S'],
    label: 'Go to Settings',
    group: 'Go to',
  },
  listNext: { hotkey: 'J', label: 'Next item', group: 'Lists' },
  listPrevious: { hotkey: 'K', label: 'Previous item', group: 'Lists' },
  listOpen: { hotkey: 'Enter', label: 'Open item', group: 'Lists' },
  rerun: { hotkey: 'R', label: 'Re-run build', group: 'This build' },
  cancel: { hotkey: 'C', label: 'Stop build', group: 'This build' },
  focusLogs: { hotkey: 'L', label: 'Focus logs', group: 'This build' },
} satisfies Record<string, ShortcutDefinition>

/** Marks the input that `/` focuses on the page. */
export const PAGE_SEARCH = { 'data-page-search': '' }

/** Marks a row's primary link as a stop for `j` / `k`. */
export const LIST_NAV_ITEM = { 'data-list-nav-item': '' }

/** Registration options shared by every single-key shortcut. */
export function shortcutOptions(definition: ShortcutDefinition) {
  return {
    // Never fire while typing in an input, textarea, select or editor.
    ignoreInputs: true,
    meta: { name: definition.label },
  }
}

/** The hotkey to register; the library's types have no `Shift+/`. */
export function registerableHotkey(
  hotkey: KeyShortcutDefinition['hotkey'],
): RegisterableHotkey {
  return hotkey === 'Shift+/' ? { key: '/', shift: true } : hotkey
}

const CATALOG = Object.values(SHORTCUTS).map((definition) => ({
  keys:
    'sequence' in definition
      ? definition.sequence.join(' ')
      : definition.hotkey,
  group: definition.group,
}))

/** The binding's group and its place in the catalog; others go last. */
function catalogPosition(keys: ReadonlyArray<string>) {
  const index = CATALOG.findIndex((item) => item.keys === keys.join(' '))
  return index === -1
    ? { group: 'General' as const, index: CATALOG.length }
    : { group: CATALOG[index].group, index }
}

export interface ShortcutEntry {
  id: string
  label: string
  /** One chord per step; a sequence such as `g h` has two. */
  steps: Array<string>
}

export interface ShortcutSection {
  group: ShortcutGroup
  entries: Array<ShortcutEntry>
}

/**
 * Groups the live registrations for the keyboard map. Registrations without
 * a name, or switched off, are not shown; duplicates collapse to one row.
 */
export function groupShortcutEntries(
  entries: ReadonlyArray<ShortcutEntry>,
): Array<ShortcutSection> {
  const seen = new Set<string>()
  const sections = new Map<ShortcutGroup, Array<ShortcutEntry>>()
  const positioned = entries
    .map((entry) => ({ entry, ...catalogPosition(entry.steps) }))
    .sort((left, right) => left.index - right.index)
  for (const { entry, group } of positioned) {
    const key = `${entry.label}\u0000${entry.steps.join(' ')}`
    if (seen.has(key)) continue
    seen.add(key)
    sections.set(group, [...(sections.get(group) ?? []), entry])
  }
  return GROUP_ORDER.flatMap((group) => {
    const groupEntries = sections.get(group)
    return groupEntries ? [{ group, entries: groupEntries }] : []
  })
}

type Platform = 'mac' | 'windows' | 'linux'

const MAC_LABELS = new Map([
  ['Mod', '⌘'],
  ['Meta', '⌘'],
  ['Control', '⌃'],
  ['Shift', '⇧'],
  ['Alt', '⌥'],
])

const OTHER_LABELS = new Map([
  ['Mod', 'Ctrl'],
  ['Meta', 'Win'],
  ['Control', 'Ctrl'],
  ['Shift', 'Shift'],
  ['Alt', 'Alt'],
])

const KEY_LABELS = new Map([
  ['Escape', 'Esc'],
  ['ArrowUp', '↑'],
  ['ArrowDown', '↓'],
  ['ArrowLeft', '←'],
  ['ArrowRight', '→'],
])

/**
 * The key caps for one chord. A bare letter is shown lower case, because
 * the shortcut does not need Shift; `Shift+/` reads as `?`.
 */
export function shortcutKeyCaps(chord: string, platform: Platform) {
  if (chord === 'Shift+/') return ['?']
  const parts = chord.split('+')
  const key = parts.at(-1) ?? chord
  const modifiers = parts.slice(0, -1)
  const labels = platform === 'mac' ? MAC_LABELS : OTHER_LABELS
  const keyLabel =
    KEY_LABELS.get(key) ??
    (modifiers.length === 0 && /^[A-Z]$/.test(key) ? key.toLowerCase() : key)
  return [
    ...modifiers.map((modifier) => labels.get(modifier) ?? modifier),
    keyLabel,
  ]
}

/** A compact hint such as `⌘K`, `g h` or `?` for palette rows. */
export function shortcutHint(
  definition: ShortcutDefinition,
  platform: Platform,
) {
  const steps =
    'sequence' in definition ? definition.sequence : [definition.hotkey]
  return steps
    .map((step) =>
      shortcutKeyCaps(step, platform).join(platform === 'mac' ? '' : '+'),
    )
    .join(' ')
}

const BUILD_NUMBER_QUERY = /^(?:(?:open|build)\s+)?#?(\d{1,9})$/i

/**
 * Reads a build number from a palette query: `#123`, `123`, `open #123` and
 * `build 123` all mean build 123.
 */
export function parseBuildNumberQuery(query: string): number | null {
  const match = BUILD_NUMBER_QUERY.exec(query.trim())
  if (!match) return null
  const number = Number(match[1])
  return number > 0 ? number : null
}

/**
 * The next row for `j` / `k`. With nothing selected yet, both start at the
 * first row; the ends do not wrap.
 */
export function stepListIndex(
  current: number,
  length: number,
  direction: 1 | -1,
): number {
  if (length === 0) return -1
  if (current < 0 || current >= length) return 0
  return Math.min(length - 1, Math.max(0, current + direction))
}
