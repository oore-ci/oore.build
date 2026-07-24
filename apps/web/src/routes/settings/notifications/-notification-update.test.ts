import { describe, expect, it } from 'vitest'

import { buildWebhookChannelUpdate } from './-notification-update'

describe('buildWebhookChannelUpdate', () => {
  it('sends an empty secret only when removal is explicitly requested', () => {
    const base = {
      enabled: true,
      events: ['failed'],
      name: 'Deploy alerts',
      url: '',
    }

    expect(
      buildWebhookChannelUpdate({ ...base, removeSecret: true, secret: '' })
        .secret,
    ).toBe('')
    expect(
      buildWebhookChannelUpdate({ ...base, removeSecret: false, secret: '' })
        .secret,
    ).toBeUndefined()
    expect(
      buildWebhookChannelUpdate({
        ...base,
        removeSecret: false,
        secret: 'replacement',
      }).secret,
    ).toBe('replacement')
  })
})
