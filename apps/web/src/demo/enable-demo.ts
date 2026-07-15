import {
  DEMO_INSTANCE_ID,
  DEMO_INSTANCE_LABEL,
  LEGACY_DEMO_AUTH_TOKEN,
  getDemoInstanceUrl,
} from './seed'
import { clearAuthStorageForInstance, useAuthStore } from '@/stores/auth-store'
import { useInstanceStore } from '@/stores/instance-store'

function seedDemoStores() {
  // Use current origin so `!!baseUrl` checks pass in query hooks and
  // The in-process demo interceptor handles full-URL requests.
  const instanceUrl = getDemoInstanceUrl()
  const now = Date.now()

  // Seed instance store — matches zustand/persist format (name: 'oore_instances')
  const demoInstance = {
    id: DEMO_INSTANCE_ID,
    label: DEMO_INSTANCE_LABEL,
    url: instanceUrl,
    addedAt: now,
  }
  const instanceStorePayload = {
    state: {
      instances: { [DEMO_INSTANCE_ID]: demoInstance },
      activeInstanceId: DEMO_INSTANCE_ID,
    },
    version: 0,
  }
  localStorage.setItem('oore_instances', JSON.stringify(instanceStorePayload))

  // The old demo always seeded an owner. Clear that one legacy session once so
  // existing visitors see the new credential-based role picker.
  if (
    localStorage.getItem(`oore_auth_token_${DEMO_INSTANCE_ID}`) ===
    LEGACY_DEMO_AUTH_TOKEN
  ) {
    clearAuthStorageForInstance(DEMO_INSTANCE_ID)
  }

  // Imperatively set Zustand store state so React sees the demo instance
  // immediately, without waiting for zustand/persist async rehydration.
  useInstanceStore.setState({
    instances: { [DEMO_INSTANCE_ID]: demoInstance },
    activeInstanceId: DEMO_INSTANCE_ID,
  })
  useAuthStore.getState().setInstanceContext(DEMO_INSTANCE_ID)
}

export async function enableDemoMode(): Promise<void> {
  seedDemoStores()

  const [{ FetchInterceptor }, { defineNetwork, InterceptorSource }] =
    await Promise.all([
      import('@mswjs/interceptors/fetch'),
      import('msw/experimental'),
    ])
  const { allHandlers } = await import('./handlers')

  const network = defineNetwork({
    handlers: allHandlers,
    sources: [
      new InterceptorSource({
        // MSW and its direct interceptor dependency expose structurally equal
        // but nominally distinct private emitter types.
        interceptors: [new FetchInterceptor() as never],
      }),
    ],
    onUnhandledFrame: ({ frame, defaults }) => {
      if (frame.protocol !== 'http') return defaults.warn()
      const request = (frame.data as { request: Request }).request
      const path = new URL(request.url).pathname
      if (path.startsWith('/v1/') || path.startsWith('/__oore_')) {
        defaults.warn()
      }
    },
  })
  await network.enable()

  // Remove the retired worker registration. A controlling legacy worker may
  // live until navigation, but fetch interception above already owns requests.
  if ('serviceWorker' in navigator) {
    const registrations = await navigator.serviceWorker.getRegistrations()
    await Promise.all(
      registrations
        .filter((registration) =>
          registration.active?.scriptURL.endsWith('/mockServiceWorker.js'),
        )
        .map((registration) => registration.unregister()),
    )
  }
}
