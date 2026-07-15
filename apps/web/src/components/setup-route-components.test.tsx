import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'

import { SetupStepIndicator } from './setup-route-components'

describe('SetupStepIndicator', () => {
  it('uses a compact current-step summary on phones', () => {
    const { rerender } = render(
      <SetupStepIndicator
        currentStep={2}
        steps={['Token', 'Mode', 'OIDC', 'Owner', 'Complete']}
      />,
    )

    const progress = screen.getByRole('navigation', {
      name: 'Setup progress',
    })
    expect(progress.firstElementChild?.textContent).toContain(
      'Step 3 of 5·OIDC',
    )
    expect(progress.querySelectorAll('[aria-current="step"]')).toHaveLength(2)

    rerender(
      <SetupStepIndicator
        currentStep={5}
        steps={['Token', 'Mode', 'OIDC', 'Owner', 'Complete']}
      />,
    )
    expect(progress.firstElementChild?.textContent).toContain(
      'Step 5 of 5·Complete',
    )
  })
})
