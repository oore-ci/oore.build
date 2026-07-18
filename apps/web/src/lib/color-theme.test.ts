import { readFileSync } from 'node:fs'
import { afterEach, describe, expect, it } from 'vitest'

import {
  COLOR_THEMES,
  COLOR_THEME_STORAGE_KEY,
  DEFAULT_COLOR_THEME,
  applyColorTheme,
  getStoredColorTheme,
  parseColorTheme,
  saveColorTheme,
} from './color-theme'

describe('color theme preference', () => {
  afterEach(() => {
    document.getElementById('oore-color-theme-vars')?.remove()
  })

  it('matches the shadcn Create Theme picker', () => {
    expect(COLOR_THEMES.map(({ name }) => name)).toEqual([
      'neutral',
      'amber',
      'blue',
      'cyan',
      'emerald',
      'fuchsia',
      'green',
      'indigo',
      'lime',
      'orange',
      'pink',
      'purple',
      'red',
      'rose',
      'sky',
      'teal',
      'violet',
      'yellow',
    ])
  })

  it('accepts supported sets and falls back to amber', () => {
    expect(parseColorTheme('amber')).toBe('amber')
    expect(parseColorTheme('unknown')).toBe(DEFAULT_COLOR_THEME)
    expect(parseColorTheme(null)).toBe(DEFAULT_COLOR_THEME)
  })

  it('reads a saved set without trusting arbitrary values', () => {
    expect(getStoredColorTheme({ getItem: () => 'green' })).toBe('green')
    expect(getStoredColorTheme({ getItem: () => 'javascript:alert(1)' })).toBe(
      DEFAULT_COLOR_THEME,
    )
  })

  it('applies and stores a selection', () => {
    const stored = new Map<string, string>()

    saveColorTheme(
      'rose',
      { setItem: (key, value) => stored.set(key, value) },
      document,
    )

    const themeSheet = document.getElementById('oore-color-theme-vars')
    expect(themeSheet?.textContent).toContain(
      '--primary: oklch(0.514 0.222 16.935)',
    )
    expect(themeSheet?.textContent).toContain(
      '.dark {\n  --background: oklch(0.145 0 0)',
    )
    expect(stored.get(COLOR_THEME_STORAGE_KEY)).toBe('rose')
  })

  it('uses one primary accent in the app and sidebar', () => {
    applyColorTheme('amber', document)

    const themeSheet = document.getElementById('oore-color-theme-vars')
    expect(
      themeSheet?.textContent.match(/--sidebar-primary: var\(--primary\);/g),
    ).toHaveLength(2)
    expect(
      themeSheet?.textContent.match(
        /--sidebar-primary-foreground: var\(--primary-foreground\);/g,
      ),
    ).toHaveLength(2)

    const staticThemeCss = readFileSync('src/styles.css', 'utf8')
    expect(
      staticThemeCss.match(/--sidebar-primary: var\(--primary\);/g),
    ).toHaveLength(2)
    expect(
      staticThemeCss.match(
        /--sidebar-primary-foreground: var\(--primary-foreground\);/g,
      ),
    ).toHaveLength(2)
  })

  it('still applies the theme when browser storage is unavailable', () => {
    saveColorTheme(
      'orange',
      {
        setItem: () => {
          throw new Error('storage disabled')
        },
      },
      document,
    )

    expect(
      document.getElementById('oore-color-theme-vars')?.textContent,
    ).toContain('--primary: oklch(0.553 0.195 38.402)')
  })

  it('can apply a stored selection before React renders', () => {
    applyColorTheme(getStoredColorTheme({ getItem: () => 'neutral' }), document)
    const themeSheet = document.getElementById('oore-color-theme-vars')
    expect(themeSheet?.textContent).toContain('--primary: oklch(0.205 0 0)')
    expect(themeSheet?.textContent).toContain(
      '--sidebar-primary: var(--primary)',
    )
    expect(themeSheet?.textContent).not.toContain(
      '--sidebar-primary: oklch(0.488 0.243 264.376)',
    )
  })
})
