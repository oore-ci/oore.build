import { describe, expect, test } from 'bun:test'

import {
  detectFeedbackDevice,
  parseFeedbackDevice,
  readFileAsBase64,
  screenshotProblem,
} from './feedback-device'

const IPHONE =
  'Mozilla/5.0 (iPhone; CPU iPhone OS 17_5_1 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.5 Mobile/15E148 Safari/604.1'
const PIXEL =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8 Build/AP2A.240805.005) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/129.0.0.0 Mobile Safari/537.36'
const REDUCED_CHROME =
  'Mozilla/5.0 (Linux; Android 10; K) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/129.0.0.0 Mobile Safari/537.36'

describe('parseFeedbackDevice', () => {
  test('reads the iPhone and iOS version', () => {
    expect(parseFeedbackDevice(IPHONE)).toEqual({
      device: 'iPhone',
      osVersion: 'iOS 17.5.1',
    })
  })

  test('reads the Android model and version', () => {
    expect(parseFeedbackDevice(PIXEL)).toEqual({
      device: 'Pixel 8',
      osVersion: 'Android 14',
    })
  })

  test('ignores the placeholder model of a reduced user agent', () => {
    expect(parseFeedbackDevice(REDUCED_CHROME)).toEqual({
      device: 'Android',
      osVersion: 'Android 10',
    })
  })

  test('leaves unknown devices empty', () => {
    expect(parseFeedbackDevice('curl/8.0')).toEqual({
      device: '',
      osVersion: '',
    })
  })
})

describe('detectFeedbackDevice', () => {
  test('prefers client hints on Android', async () => {
    const device = await detectFeedbackDevice({
      userAgent: REDUCED_CHROME,
      userAgentData: {
        getHighEntropyValues: () =>
          Promise.resolve({
            platform: 'Android',
            model: 'SM-S918B',
            platformVersion: '15.0.0',
          }),
      },
    })
    expect(device).toEqual({ device: 'SM-S918B', osVersion: 'Android 15' })
  })

  test('falls back to the user agent when hints fail', async () => {
    const device = await detectFeedbackDevice({
      userAgent: PIXEL,
      userAgentData: {
        getHighEntropyValues: () => Promise.reject(new Error('blocked')),
      },
    })
    expect(device).toEqual({ device: 'Pixel 8', osVersion: 'Android 14' })
  })
})

describe('screenshotProblem', () => {
  test('accepts small raster images only', () => {
    expect(screenshotProblem({ size: 1000, type: 'image/png' })).toBeNull()
    expect(screenshotProblem({ size: 1000, type: 'image/svg+xml' })).toBe(
      'Choose a PNG, JPEG or WebP image.',
    )
    expect(
      screenshotProblem({ size: 6 * 1024 * 1024, type: 'image/jpeg' }),
    ).toBe('Choose an image of 5 MB or less.')
  })
})

describe('readFileAsBase64', () => {
  test('encodes files larger than one chunk', async () => {
    const bytes = new Uint8Array(70_000).map((_, index) => index % 251)
    expect(await readFileAsBase64(new Blob([bytes]))).toBe(
      Buffer.from(bytes).toString('base64'),
    )
  })
})
