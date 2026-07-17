import { render, screen } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { SetupRouteError, SetupStepIndicator } from './setup-route-components'
import { useInstanceStore } from '@/stores/instance-store'

const { navigate, invalidate } = vi.hoisted(() => ({
  navigate: vi.fn(),
  invalidate: vi.fn(),
}))

vi.mock('@tanstack/react-router', () => ({
  useNavigate: () => navigate,
  useRouter: () => ({ invalidate }),
}))

vi.mock('@/lib/connectivity', () => {
  return {
    getConnectivityIssue: () => ({
      kind: 'network_unreachable',
      title: 'Backend unreachable',
      description: 'Could not reach the backend.',
    }),
    isHostedUiOrigin: () => true,
  }
})

beforeEach(() => {
  navigate.mockReset()
  invalidate.mockReset()
  useInstanceStore.setState({ instances: {}, activeInstanceId: null })
})

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

describe('SetupRouteError', () => {
  it('quotes a crafted backend as one argument in both shell commands', () => {
    const id = 'crafted-instance'
    const backendUrl =
      "https://backend.invalid/path/$(SUBSTITUTION);`BACKTICK`'"
    const quotedBackendUrl =
      "'https://backend.invalid/path/$(SUBSTITUTION);`BACKTICK`'\"'\"''"
    useInstanceStore.setState({
      activeInstanceId: id,
      instances: {
        [id]: {
          id,
          label: 'Crafted',
          url: backendUrl,
          addedAt: 1,
        },
      },
    })

    render(<SetupRouteError error={new Error('Failed to fetch')} />)

    expect(
      screen.getByText(`cloudflared tunnel --url ${quotedBackendUrl}`),
    ).toBeTruthy()
    expect(
      screen.getByText(`oore-web --backend-url ${quotedBackendUrl}`),
    ).toBeTruthy()
    expect(
      screen.queryByText(`cloudflared tunnel --url ${backendUrl}`),
    ).toBeNull()
    expect(
      screen.queryByText(`oore-web --backend-url ${backendUrl}`),
    ).toBeNull()
  })

  it('keeps ordinary backend URLs usable in both commands', () => {
    const id = 'ordinary-instance'
    useInstanceStore.setState({
      activeInstanceId: id,
      instances: {
        [id]: {
          id,
          label: 'Ordinary',
          url: 'https://ci.example.com',
          addedAt: 1,
        },
      },
    })

    render(<SetupRouteError error={new Error('Failed to fetch')} />)

    expect(
      screen.getByText("cloudflared tunnel --url 'https://ci.example.com'"),
    ).toBeTruthy()
    expect(
      screen.getByText("oore-web --backend-url 'https://ci.example.com'"),
    ).toBeTruthy()
  })
})
