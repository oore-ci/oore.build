import { fireEvent, render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import { CollectionSearchInput } from './collection-search-input'

describe('CollectionSearchInput', () => {
  it('keeps focus when the URL-backed value catches up and resets in place', () => {
    const { rerender } = render(
      <CollectionSearchInput
        ariaLabel="Search repositories"
        initialValue=""
        onSearch={vi.fn()}
        placeholder="Search repositories"
      />,
    )
    const input = screen.getByRole('searchbox')
    input.focus()
    expect(document.activeElement).toBe(input)
    fireEvent.change(input, { target: { value: 'native' } })

    rerender(
      <CollectionSearchInput
        ariaLabel="Search repositories"
        initialValue="native"
        onSearch={vi.fn()}
        placeholder="Search repositories"
      />,
    )
    expect(screen.getByRole('searchbox')).toBe(input)
    expect(document.activeElement).toBe(input)

    rerender(
      <CollectionSearchInput
        ariaLabel="Search repositories"
        initialValue=""
        onSearch={vi.fn()}
        placeholder="Search repositories"
      />,
    )
    expect((screen.getByRole('searchbox') as HTMLInputElement).value).toBe('')
  })
})
