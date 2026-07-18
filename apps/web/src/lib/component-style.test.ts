import { beforeEach, describe, expect, it, vi } from 'vitest'

import {
  COMPONENT_STYLES,
  DEFAULT_COMPONENT_STYLE,
  applyComponentStyle,
  getStoredComponentStyle,
  parseComponentStyle,
  saveComponentStyle,
} from './component-style'

describe('component style', () => {
  beforeEach(() => {
    document.body.className = 'app-shell style-vega'
    document.documentElement.style.removeProperty('--radius')
  })

  it('matches the current shadcn Create style picker', () => {
    expect(COMPONENT_STYLES.map((style) => style.name)).toEqual([
      'vega',
      'nova',
      'maia',
      'lyra',
      'mira',
      'luma',
      'sera',
      'rhea',
    ])
  })

  it('fails closed to Vega for unknown stored values', () => {
    expect(parseComponentStyle('nova')).toBe('nova')
    expect(parseComponentStyle('javascript:alert(1)')).toBe(
      DEFAULT_COMPONENT_STYLE,
    )
    expect(getStoredComponentStyle({ getItem: () => 'unknown' })).toBe(
      DEFAULT_COMPONENT_STYLE,
    )
  })

  it('replaces only the managed body style class and persists it', async () => {
    const setItem = vi.fn()

    await saveComponentStyle('mira', { setItem }, document)

    expect(document.body.classList.contains('style-vega')).toBe(false)
    expect(document.body.classList.contains('style-mira')).toBe(true)
    expect(document.body.classList.contains('app-shell')).toBe(true)
    expect(document.documentElement.style.getPropertyValue('--radius')).toBe(
      '0.625rem',
    )
    expect(setItem).toHaveBeenCalledWith('oore_component_style', 'mira')

    applyComponentStyle('invalid', document)
    expect(document.body.classList.contains('style-vega')).toBe(true)
    expect(document.body.classList.contains('style-mira')).toBe(false)
  })

  it('applies the official Create radius constraints with the style', () => {
    applyComponentStyle('lyra', document)
    expect(document.documentElement.style.getPropertyValue('--radius')).toBe(
      '0rem',
    )

    applyComponentStyle('sera', document)
    expect(document.documentElement.style.getPropertyValue('--radius')).toBe(
      '0rem',
    )

    applyComponentStyle('rhea', document)
    expect(document.documentElement.style.getPropertyValue('--radius')).toBe(
      '0.625rem',
    )
  })
})
