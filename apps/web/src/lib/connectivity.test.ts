import { describe, expect, it } from 'vitest'
import {
  getConnectivityIssue,
  isHostedUiOrigin,
  isLocalLauncherOrigin,
  isLoopbackUrl,
  isMixedContentBlocked,
} from '@/lib/connectivity'

describe('connectivity helpers', () => {
  it('detects hosted ui origin', () => {
    expect(isHostedUiOrigin('https://ci.oore.build')).toBe(true)
    expect(isHostedUiOrigin('http://localhost:3000')).toBe(false)
  })

  it('detects local launcher origin and loopback urls', () => {
    expect(isLocalLauncherOrigin('http://127.0.0.1:4173')).toBe(true)
    expect(isLocalLauncherOrigin('http://localhost:4173')).toBe(true)
    expect(isLocalLauncherOrigin('http://localhost:3000')).toBe(false)

    expect(isLoopbackUrl('http://127.0.0.1:8787')).toBe(true)
    expect(isLoopbackUrl('http://localhost:8787')).toBe(true)
    expect(isLoopbackUrl('https://ci.example.com')).toBe(false)
  })

  it('detects mixed-content combinations', () => {
    expect(
      isMixedContentBlocked('https://ci.oore.build', 'http://127.0.0.1:8787'),
    ).toBe(true)
    expect(
      isMixedContentBlocked('http://localhost:3000', 'http://127.0.0.1:8787'),
    ).toBe(false)
    expect(
      isMixedContentBlocked('https://ci.oore.build', 'https://ci.example.com'),
    ).toBe(false)
  })

  it('returns a mixed-content issue when protocol pairing is blocked', () => {
    const issue = getConnectivityIssue(
      'http://127.0.0.1:8787',
      new Error('Failed to fetch'),
      'https://ci.oore.build',
    )
    expect(issue?.kind).toBe('mixed_content')
  })

  it('returns network-unreachable issue for fetch network failures', () => {
    const issue = getConnectivityIssue(
      'https://ci.example.com',
      new Error('Failed to fetch'),
      'https://ci.oore.build',
    )
    expect(issue?.kind).toBe('network_unreachable')
  })
})
