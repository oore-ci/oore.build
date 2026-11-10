import { describe, expect, test } from 'bun:test'

import { artifactPurposeLabel, detectInstallDevice } from './artifact-install'

const IOS =
  'Mozilla/5.0 (iPhone; CPU iPhone OS 17_5_1 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko)'
const SAFARI = `${IOS} Version/17.5 Mobile/15E148 Safari/604.1`
const CHROME = `${IOS} CriOS/129.0.6668.69 Mobile/15E148 Safari/604.1`
const FIREFOX = `${IOS} FxiOS/130.0 Mobile/15E148 Safari/605.1.15`
const EDGE = `${IOS} Version/17.0 EdgiOS/128.0.2739.60 Mobile/15E148 Safari/604.1`
const WKWEBVIEW = `${IOS} Mobile/15E148`
const GMAIL = `${IOS} Mobile/15E148 GSA/335.0.669042397`
const FACEBOOK = `${IOS} Mobile/15E148 [FBAN/FBIOS;FBAV/480.0.0.38.109;FBBV/640214040;FBDV/iPhone15,2]`
const ANDROID =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/129.0.0.0 Mobile Safari/537.36'
const MAC =
  'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.5 Safari/605.1.15'

describe('detectInstallDevice', () => {
  test('treats Safari and full third-party browsers alike', () => {
    for (const userAgent of [SAFARI, CHROME, FIREFOX, EDGE]) {
      expect(detectInstallDevice(userAgent)).toBe('iphone')
    }
  })

  test('flags browsers embedded in other apps', () => {
    for (const userAgent of [WKWEBVIEW, GMAIL, FACEBOOK]) {
      expect(detectInstallDevice(userAgent)).toBe('iphone-webview')
    }
  })

  test('tells Android and desktop apart', () => {
    expect(detectInstallDevice(ANDROID)).toBe('android')
    expect(detectInstallDevice(MAC)).toBe('other')
    expect(detectInstallDevice('')).toBe('other')
  })
})

describe('artifactPurposeLabel', () => {
  test('names the app by platform and variant', () => {
    expect(
      artifactPurposeLabel({ artifact_type: 'apk', name: 'app-debug.apk' }),
    ).toBe('Android debug app')
    expect(
      artifactPurposeLabel({
        artifact_type: 'apk',
        name: 'app-arm64-v8a-release.apk',
      }),
    ).toBe('Android app')
    expect(
      artifactPurposeLabel({ artifact_type: 'ipa', name: 'FlutterShop.ipa' }),
    ).toBe('iOS app')
  })

  test('says an App Bundle cannot be installed', () => {
    expect(
      artifactPurposeLabel({ artifact_type: 'aab', name: 'app-release.aab' }),
    ).toBe('App Bundle for Google Play, not installable')
  })
})
