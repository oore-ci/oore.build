import { isDemoMode } from '@/lib/demo-mode'

/** The service worker that shows pushes; it caches nothing. */
export const PUSH_WORKER_URL = '/sw.js'

/**
 * Push needs a service worker and the Push API. On iPhone that means Oore
 * added to the Home Screen. The demo's mock server owns the only worker.
 */
export function webPushSupported(): boolean {
  return (
    !isDemoMode &&
    'serviceWorker' in globalThis.navigator &&
    'PushManager' in globalThis &&
    'Notification' in globalThis
  )
}

/** A base64url VAPID key as the bytes `pushManager.subscribe` takes. */
export function vapidKeyBytes(base64url: string): Uint8Array<ArrayBuffer> {
  const padded = base64url
    .replaceAll('-', '+')
    .replaceAll('_', '/')
    .padEnd(Math.ceil(base64url.length / 4) * 4, '=')
  const binary = atob(padded)
  const bytes = new Uint8Array(new ArrayBuffer(binary.length))
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i)
  return bytes
}

/** The keys the server encrypts each push with, from a subscription. */
export function subscriptionKeys(
  subscription: PushSubscription,
): { endpoint: string; p256dh: string; auth: string } | null {
  const json = subscription.toJSON()
  const p256dh = json.keys?.p256dh
  const auth = json.keys?.auth
  if (!json.endpoint || !p256dh || !auth) return null
  return { endpoint: json.endpoint, p256dh, auth }
}

export async function currentPushSubscription(): Promise<PushSubscription | null> {
  if (!webPushSupported()) return null
  const registration =
    await navigator.serviceWorker.getRegistration(PUSH_WORKER_URL)
  return (await registration?.pushManager.getSubscription()) ?? null
}

/** Asks for permission, registers the worker and subscribes this browser. */
export async function subscribeThisBrowser(
  publicKey: string,
): Promise<PushSubscription> {
  const permission = await Notification.requestPermission()
  if (permission !== 'granted') {
    throw new Error(
      'Notifications are blocked for Oore. Allow them in the site settings.',
    )
  }
  const registration = await navigator.serviceWorker.register(PUSH_WORKER_URL)
  await navigator.serviceWorker.ready
  const existing = await registration.pushManager.getSubscription()
  if (existing) return existing
  return registration.pushManager.subscribe({
    userVisibleOnly: true,
    applicationServerKey: vapidKeyBytes(publicKey),
  })
}
