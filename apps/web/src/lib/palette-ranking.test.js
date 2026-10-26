import { describe, expect, test } from 'bun:test'

import { rankPaletteSections } from './palette-ranking'

// Scores by how early the query starts in the value; 0 when absent.
function scoreByPosition(value, search, keywords = []) {
  const text = [value, ...keywords].join(' ').toLowerCase()
  const index = text.indexOf(search)
  if (index !== -1) return 1 / (index + 2)
  return [...search].every((char) => text.includes(char)) ? 0.01 : 0
}

const sections = [
  {
    heading: 'Actions',
    items: [
      { value: 'Run build… action-run-build', keywords: ['run pipeline'] },
    ],
  },
  {
    heading: 'Run pipeline',
    items: [
      { value: 'Run Android Release run-1' },
      { value: 'Run iOS Release run-2' },
    ],
  },
  { heading: 'Theme', items: [{ value: 'Use dark theme theme-dark' }] },
]

describe('rankPaletteSections', () => {
  test('keeps the authored order without a query', () => {
    expect(
      rankPaletteSections(sections, '  ', scoreByPosition).map(
        (section) => section.heading,
      ),
    ).toEqual(['Actions', 'Run pipeline', 'Theme'])
  })

  test('drops misses and ranks rows that contain the query first', () => {
    const ranked = rankPaletteSections(sections, 'Run iOS', scoreByPosition)

    expect(ranked.map((section) => section.heading)).toEqual(['Run pipeline'])
    expect(ranked[0].items.map((item) => item.value)).toEqual([
      'Run iOS Release run-2',
      'Run Android Release run-1',
    ])
  })

  test('orders sections by their best row', () => {
    const ranked = rankPaletteSections(sections, 'run', scoreByPosition)

    expect(ranked.map((section) => section.heading)).toEqual([
      'Actions',
      'Run pipeline',
    ])
    expect(
      rankPaletteSections(sections, 'theme', scoreByPosition)[0].heading,
    ).toBe('Theme')
  })
})
