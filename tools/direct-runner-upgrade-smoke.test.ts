import { describe, expect, test } from 'bun:test'

import {
  parseTriggerMode,
  runnerMatchesUpgrade,
  selectRunner,
  type Runner,
} from './direct-runner-upgrade-smoke'

const runner = (overrides: Partial<Runner> = {}): Runner => ({
  id: 'runner-1',
  name: 'mac-builder',
  status: 'online',
  last_heartbeat_at: 100,
  capabilities: { version: '1.2.3-alpha.2', protocol_version: 4 },
  ...overrides,
})

describe('direct runner upgrade smoke', () => {
  test('supports API-triggered and literal UI-observation runs', () => {
    expect(parseTriggerMode()).toBe('api')
    expect(parseTriggerMode(' observe ')).toBe('observe')
    expect(() => parseTriggerMode('launchctl')).toThrow('api or observe')
  })

  test('selects one online runner or an explicit runner', () => {
    const offline = runner({ id: 'offline', status: 'offline' })
    const online = runner()
    expect(selectRunner([offline, online])).toBe(online)
    expect(selectRunner([offline, online], 'offline')).toBe(offline)
    expect(() => selectRunner([online, runner({ id: 'runner-2' })])).toThrow(
      'set OORE_UPGRADE_SMOKE_RUNNER_ID',
    )
  })

  test('requires the restarted runner identity reported by the APIs', () => {
    expect(runnerMatchesUpgrade(runner(), 99, '1.2.3-alpha.2')).toBe(true)
    expect(runnerMatchesUpgrade(runner(), 100, '1.2.3-alpha.2')).toBe(false)
    expect(
      runnerMatchesUpgrade(
        runner({
          capabilities: { version: '1.2.3-alpha.1', protocol_version: 4 },
        }),
        99,
        '1.2.3-alpha.2',
      ),
    ).toBe(false)
    expect(
      runnerMatchesUpgrade(
        runner({
          capabilities: { version: '1.2.3-alpha.2', protocol_version: 3 },
        }),
        99,
        '1.2.3-alpha.2',
      ),
    ).toBe(false)
  })
})
