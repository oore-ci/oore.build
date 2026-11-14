import assert from 'node:assert/strict'
import { test } from 'bun:test'

import { plainRefusal, plainRefusals, reviewGuidelines } from './store-refusals'

const appStore = {
  store: 'app_store',
  console_url:
    'https://appstoreconnect.apple.com/apps/123/distribution/ios/version/inflight',
}
const play = {
  store: 'google_play',
  console_url: 'https://play.google.com/apps/publish/?package=com.acme',
}

test('App Store refusals point at the page that fixes them', () => {
  const age = plainRefusal(
    'ENTITY_ERROR.ATTRIBUTE.REQUIRED: You must provide a value for ageRatingDeclaration',
    appStore,
  )
  assert.equal(age.code, 'age_rating')
  assert.equal(
    age.href,
    'https://appstoreconnect.apple.com/apps/123/distribution/info',
  )
  const notes = plainRefusal("The 'What's New' field is required", appStore)
  assert.equal(notes.code, 'whats_new')
  assert.equal(notes.href, appStore.console_url)
  assert.equal(
    plainRefusal('App Review contact details are missing', appStore).code,
    'review_details',
  )
})

test('Play refusals', () => {
  assert.equal(
    plainRefusal(
      'Changes cannot be sent for review automatically. Please set the query parameter changesNotSentForReview to true.',
      play,
    ).code,
    'changes_not_sent_for_review',
  )
  assert.equal(
    plainRefusal('The caller does not have permission', play).href,
    'https://play.google.com/console/users-and-permissions',
  )
  // App Store-only rules never match Play.
  assert.equal(plainRefusal('export compliance missing', play), null)
})

test('unknown or empty text has no plain version', () => {
  assert.equal(plainRefusal('Something odd happened', appStore), null)
  assert.equal(plainRefusal(undefined, appStore), null)
})

test('guidelines named in a rejection', () => {
  assert.deepEqual(
    reviewGuidelines(
      'Guideline 2.1 - Performance. Guideline 5.1.1 - Privacy. guideline 2.1 again',
    ),
    ['2.1', '5.1.1'],
  )
})

test('a refusal naming several problems lists each', () => {
  const found = plainRefusals(
    "- An age rating declaration update is required.\n- You must provide a value for the attribute 'whatsNew'.",
    appStore,
  )
  assert.deepEqual(
    found.map((item) => item.code),
    ['whats_new', 'age_rating'],
  )
})
