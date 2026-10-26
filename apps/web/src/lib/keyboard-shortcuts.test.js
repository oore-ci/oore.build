import { describe, expect, test } from 'bun:test'

import {
  SHORTCUTS,
  groupShortcutEntries,
  parseBuildNumberQuery,
  shortcutHint,
  shortcutKeyCaps,
  stepListIndex,
} from './keyboard-shortcuts'

describe('parseBuildNumberQuery', () => {
  test('reads the forms people type to open a build', () => {
    expect(parseBuildNumberQuery('#123')).toBe(123)
    expect(parseBuildNumberQuery('123')).toBe(123)
    expect(parseBuildNumberQuery('open #123')).toBe(123)
    expect(parseBuildNumberQuery('  Build 42 ')).toBe(42)
  })

  test('ignores anything that is not a build number', () => {
    expect(parseBuildNumberQuery('')).toBeNull()
    expect(parseBuildNumberQuery('#0')).toBeNull()
    expect(parseBuildNumberQuery('run android')).toBeNull()
    expect(parseBuildNumberQuery('open #12a')).toBeNull()
    expect(parseBuildNumberQuery('#1234567890')).toBeNull()
  })
})

describe('shortcutKeyCaps', () => {
  test('uses symbols on macOS and words elsewhere', () => {
    expect(shortcutKeyCaps('Mod+K', 'mac')).toEqual(['⌘', 'K'])
    expect(shortcutKeyCaps('Mod+K', 'linux')).toEqual(['Ctrl', 'K'])
  })

  test('shows bare letters lower case and Shift+/ as ?', () => {
    expect(shortcutKeyCaps('R', 'mac')).toEqual(['r'])
    expect(shortcutKeyCaps('Shift+/', 'windows')).toEqual(['?'])
    expect(shortcutKeyCaps('Enter', 'mac')).toEqual(['Enter'])
  })

  test('joins sequences into a palette hint', () => {
    expect(shortcutHint(SHORTCUTS.goBuilds, 'mac')).toBe('g b')
    expect(shortcutHint(SHORTCUTS.palette, 'linux')).toBe('Ctrl+K')
  })
})

describe('groupShortcutEntries', () => {
  test('orders sections and rows by the catalog, dropping duplicates', () => {
    const sections = groupShortcutEntries([
      { id: '1', label: 'Re-run build', steps: ['R'] },
      { id: '6', label: 'Toggle dark mode', steps: ['D'] },
      { id: '2', label: 'Go to Builds', steps: ['G', 'B'] },
      { id: '3', label: 'Open Quick open', steps: ['Mod+K'] },
      { id: '4', label: 'Go to Builds', steps: ['G', 'B'] },
      { id: '5', label: 'Switch to Demo', steps: ['Mod+1'] },
    ])

    expect(sections.map((section) => section.group)).toEqual([
      'General',
      'Go to',
      'This build',
    ])
    expect(sections[0].entries.map((entry) => entry.label)).toEqual([
      'Open Quick open',
      'Toggle dark mode',
      'Switch to Demo',
    ])
    expect(sections[1].entries).toHaveLength(1)
  })
})

describe('stepListIndex', () => {
  test('starts at the first row and stops at the ends', () => {
    expect(stepListIndex(-1, 3, 1)).toBe(0)
    expect(stepListIndex(-1, 3, -1)).toBe(0)
    expect(stepListIndex(0, 3, 1)).toBe(1)
    expect(stepListIndex(2, 3, 1)).toBe(2)
    expect(stepListIndex(0, 3, -1)).toBe(0)
    expect(stepListIndex(0, 0, 1)).toBe(-1)
  })
})
