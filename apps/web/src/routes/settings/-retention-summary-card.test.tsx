import { fireEvent, render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import { RetentionSummaryCard } from './-retention-summary-card'

describe('RetentionSummaryCard', () => {
  it('reports a failed cleanup query without claiming no cleanup has run', () => {
    const onRetry = vi.fn()

    render(
      <RetentionSummaryCard
        error={new Error('service unavailable')}
        isLoading={false}
        lastCleanup={undefined}
        onRetry={onRetry}
      />,
    )

    expect(screen.getByText(/Failed to load the last cleanup/)).toBeTruthy()
    expect(screen.queryByText(/No cleanup has run yet/)).toBeNull()
    fireEvent.click(screen.getByRole('button', { name: 'Retry' }))
    expect(onRetry).toHaveBeenCalledOnce()
  })
})
