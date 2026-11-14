import assert from 'node:assert/strict'
import { test } from 'bun:test'

import { vapidKeyBytes } from './web-push'

test('vapidKeyBytes decodes unpadded base64url', () => {
  assert.deepEqual([...vapidKeyBytes('AQID_-8')], [1, 2, 3, 255, 239])
})
