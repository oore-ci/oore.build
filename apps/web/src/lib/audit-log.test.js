import { describe, expect, test } from 'bun:test'

import {
  auditInitials,
  describeAuditEntry,
  groupAuditEntries,
  parseAuditDetails,
} from './audit-log'

function entry(overrides) {
  return {
    id: 1,
    actor_id: 'u1',
    actor_email: 'alex@example.com',
    actor_name: 'Alex Chen',
    action: 'build_created',
    category: 'builds',
    resource_type: 'build',
    resource_id: 'b1',
    details: null,
    names: {},
    created_at: 1_794_787_200,
    ...overrides,
  }
}

function sentence(description) {
  return `${description.actor} ${description.say.map((part) => part.text).join('')}`
}

describe('describeAuditEntry', () => {
  test('names the build by its app, pipeline and number', () => {
    const description = describeAuditEntry(
      entry({
        action: 'artifact_install_link_generated',
        resource_type: 'artifact_download_token',
        project_name: 'Shop',
        pipeline_name: 'iOS Release',
        build_number: 482,
        details: JSON.stringify({ platform: 'ios', token_prefix: 'bbd83621' }),
      }),
    )
    expect(sentence(description)).toBe(
      'Alex Chen created an install link for Shop iOS Release #482',
    )
    expect(description.say.at(-1).strong).toBe(true)
    expect(description.meta).toBe('iOS')
    expect(description.facts).toContainEqual({
      label: 'Token starts with',
      value: 'bbd83621',
      mono: true,
    })
  })

  test('falls back to plain words when the record was deleted', () => {
    const description = describeAuditEntry(entry({ action: 'build_canceled' }))
    expect(sentence(description)).toBe('Alex Chen canceled a build')
    expect(description.say.at(-1).strong).toBeUndefined()
  })

  test('resolves people and roles from details', () => {
    const description = describeAuditEntry(
      entry({
        action: 'role_changed',
        resource_type: 'user',
        resource_id: 'u2',
        names: { u2: 'Taylor Ruiz' },
        details: JSON.stringify({
          from_role: 'qa_viewer',
          to_role: 'developer',
          changed_by: 'alex@example.com',
        }),
      }),
    )
    expect(sentence(description)).toBe(
      'Alex Chen changed Taylor Ruiz from QA Viewer to Developer',
    )
    // Who changed it is the actor, so it is not repeated as a fact.
    expect(description.facts.map((fact) => fact.label)).not.toContain(
      'Changed by',
    )
  })

  test('an entry nobody made is Oore, and a blocked one is someone unknown', () => {
    const cleanup = describeAuditEntry(
      entry({
        actor_id: null,
        actor_email: null,
        actor_name: null,
        action: 'artifact_expiry_cleanup_completed',
        details: JSON.stringify({ artifacts_deleted: 0, bytes_reclaimed: 0 }),
      }),
    )
    expect(sentence(cleanup)).toBe('Oore cleaned up expired artifacts')
    expect(cleanup.meta).toBe('Nothing to delete')
    expect(cleanup.problem).toBeNull()

    const blocked = describeAuditEntry(
      entry({
        actor_id: null,
        actor_email: null,
        actor_name: null,
        action: 'trusted_proxy_login_blocked_untrusted_peer',
        details: JSON.stringify({ peer_ip: '203.0.113.44' }),
      }),
    )
    expect(blocked.actor).toBe('Someone')
    expect(blocked.actorKind).toBe('unknown')
    expect(blocked.problem).toBe('Blocked')
    expect(blocked.meta).toBe('203.0.113.44')
  })

  test('reads the older name=value details', () => {
    const description = describeAuditEntry(
      entry({
        action: 'api_token_created',
        resource_type: 'api_token',
        details: 'name=ci-deploy, role=developer, prefix=oore_7f3a',
      }),
    )
    expect(sentence(description)).toBe(
      'Alex Chen created the API token ci-deploy',
    )
    expect(description.meta).toBe('Developer role · Starts with oore_7f3a')
  })

  test('an action without a template still reads as a sentence', () => {
    expect(
      sentence(describeAuditEntry(entry({ action: 'widget_rotated' }))),
    ).toBe('Alex Chen rotated widget')
    expect(
      describeAuditEntry(
        entry({ action: 'something_new', details: 'Free text' }),
      ).meta,
    ).toBe('Free text')
  })
})

describe('parseAuditDetails', () => {
  test('keeps free text as a note', () => {
    expect(parseAuditDetails('Manual trigger on main')).toEqual({
      fields: new Map(),
      note: 'Manual trigger on main',
    })
    expect(parseAuditDetails(null)).toEqual({ fields: new Map(), note: null })
  })
})

describe('groupAuditEntries', () => {
  test('splits by day and folds repeated housekeeping into one row', () => {
    const noon = new Date(2026, 10, 16, 12).getTime() / 1000
    const cleanup = (id, at) =>
      entry({
        id,
        actor_id: null,
        action: 'artifact_expiry_cleanup_completed',
        category: 'system',
        created_at: at,
      })
    const days = groupAuditEntries([
      cleanup(5, noon),
      entry({ id: 4, created_at: noon - 3600 }),
      cleanup(3, noon - 4 * 3600),
      entry({ id: 2, created_at: noon - 5 * 3600 }),
      cleanup(1, noon - 86_400),
    ])

    expect(days.map((day) => day.date)).toEqual(['2026-11-16', '2026-11-15'])
    expect(days[0].rows.map((row) => row.entry.id)).toEqual([5, 4, 2])
    expect(days[0].rows[0].repeats.map((repeat) => repeat.id)).toEqual([3])
    expect(days[1].rows[0].repeats).toEqual([])
  })
})

test('auditInitials', () => {
  expect(auditInitials('Alex Chen')).toBe('AC')
  expect(auditInitials('aryakumar.jha@example.com')).toBe('AJ')
  expect(auditInitials('sam@example.com')).toBe('SA')
})
