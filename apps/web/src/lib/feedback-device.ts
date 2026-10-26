/** What a feedback form can tell about the tester's device. */
export interface FeedbackDevice {
  device: string
  osVersion: string
}

export const FEEDBACK_MAX_MESSAGE_CHARS = 4000
export const FEEDBACK_MAX_FIELD_CHARS = 120
export const FEEDBACK_MAX_SCREENSHOT_BYTES = 5 * 1024 * 1024
export const FEEDBACK_SCREENSHOT_TYPES = [
  'image/png',
  'image/jpeg',
  'image/webp',
] as const

const SCREENSHOT_TYPES = new Set<string>(FEEDBACK_SCREENSHOT_TYPES)

// Chrome's reduced user agent reports this placeholder instead of the model.
const PLACEHOLDER_ANDROID_MODELS = new Set(['K', 'wv'])

function dotted(version: string) {
  return version.replaceAll('_', '.')
}

/** Best guess at the device and OS from a user agent string. */
export function parseFeedbackDevice(userAgent: string): FeedbackDevice {
  const apple = /\b(iPhone|iPad|iPod)\b/.exec(userAgent)
  if (apple) {
    const version = /\bOS (\d+(?:[_.]\d+){0,2}) like Mac OS X/.exec(userAgent)
    const os = apple[1] === 'iPad' ? 'iPadOS' : 'iOS'
    return {
      device: apple[1],
      osVersion: version ? `${os} ${dotted(version[1])}` : os,
    }
  }

  const android = /\bAndroid (\d+(?:\.\d+)*)?/.exec(userAgent)
  if (android) {
    const model = /\bAndroid[^;)]*;\s*([^;)]+?)(?:\s+Build\/[^;)]*)?\s*[;)]/
      .exec(userAgent)?.[1]
      ?.trim()
    return {
      device:
        model && !PLACEHOLDER_ANDROID_MODELS.has(model) ? model : 'Android',
      osVersion: android[1] ? `Android ${android[1]}` : 'Android',
    }
  }

  const mac = /\bMac OS X (\d+(?:[_.]\d+){0,2})/.exec(userAgent)
  if (mac) return { device: 'Mac', osVersion: `macOS ${dotted(mac[1])}` }
  if (/\bWindows\b/.test(userAgent))
    return { device: 'Windows PC', osVersion: 'Windows' }
  if (/\bLinux\b/.test(userAgent)) return { device: '', osVersion: 'Linux' }
  return { device: '', osVersion: '' }
}

interface HighEntropyValues {
  model?: string
  platform?: string
  platformVersion?: string
}

interface UserAgentDataLike {
  getHighEntropyValues?: (hints: Array<string>) => Promise<HighEntropyValues>
}

/**
 * Chrome on Android hides the model and full OS version from the user agent
 * string; ask for them through User-Agent Client Hints where available.
 */
export async function detectFeedbackDevice(
  nav: { userAgent: string; userAgentData?: UserAgentDataLike } | undefined,
): Promise<FeedbackDevice> {
  const parsed = parseFeedbackDevice(nav?.userAgent ?? '')
  const getHighEntropyValues = nav?.userAgentData?.getHighEntropyValues
  if (!getHighEntropyValues) return parsed
  try {
    const values = await getHighEntropyValues.call(nav.userAgentData, [
      'model',
      'platformVersion',
    ])
    if (values.platform !== 'Android') return parsed
    const major = values.platformVersion?.split('.')[0]
    return {
      device: values.model?.trim() || parsed.device,
      osVersion: major ? `Android ${major}` : parsed.osVersion,
    }
  } catch {
    return parsed
  }
}

/** Why a screenshot cannot be sent, or `null` when it can. */
export function screenshotProblem(file: {
  size: number
  type: string
}): string | null {
  if (!SCREENSHOT_TYPES.has(file.type))
    return 'Choose a PNG, JPEG or WebP image.'
  if (file.size > FEEDBACK_MAX_SCREENSHOT_BYTES)
    return 'Choose an image of 5 MB or less.'
  return null
}

/** Base64 file contents, as the feedback API takes a screenshot. */
export async function readFileAsBase64(file: Blob): Promise<string> {
  const bytes = new Uint8Array(await file.arrayBuffer())
  let binary = ''
  const chunk = 0x8000
  for (let index = 0; index < bytes.length; index += chunk) {
    binary += String.fromCharCode(...bytes.subarray(index, index + chunk))
  }
  return btoa(binary)
}
