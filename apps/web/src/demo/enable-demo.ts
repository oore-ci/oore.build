import {
  DEMO_AUTH_EXPIRES_AT,
  DEMO_AUTH_TOKEN,
  DEMO_INSTANCE_ID,
  DEMO_INSTANCE_LABEL,
  DEMO_OIDC_SUBJECT,
  DEMO_USER_EMAIL,
  DEMO_USER_ID,
  DEMO_USER_ROLE,
  getDemoInstanceUrl,
} from './seed'

function seedDemoStores() {
  // Use current origin so `!!baseUrl` checks pass in query hooks and
  // MSW intercepts the full-URL fetches before they hit the network.
  const instanceUrl = getDemoInstanceUrl()

  // Seed instance store — matches zustand/persist format (name: 'oore_instances')
  const instanceStorePayload = {
    state: {
      instances: {
        [DEMO_INSTANCE_ID]: {
          id: DEMO_INSTANCE_ID,
          label: DEMO_INSTANCE_LABEL,
          url: instanceUrl,
          addedAt: Date.now(),
        },
      },
      activeInstanceId: DEMO_INSTANCE_ID,
    },
    version: 0,
  }
  localStorage.setItem('oore_instances', JSON.stringify(instanceStorePayload))

  // Seed auth store — matches key pattern from auth-store.ts
  localStorage.setItem(`oore_auth_token_${DEMO_INSTANCE_ID}`, DEMO_AUTH_TOKEN)
  localStorage.setItem(
    `oore_auth_expires_${DEMO_INSTANCE_ID}`,
    String(DEMO_AUTH_EXPIRES_AT),
  )
  localStorage.setItem(
    `oore_auth_user_${DEMO_INSTANCE_ID}`,
    JSON.stringify({
      email: DEMO_USER_EMAIL,
      oidc_subject: DEMO_OIDC_SUBJECT,
      user_id: DEMO_USER_ID,
      role: DEMO_USER_ROLE,
    }),
  )

  // Seed last auth meta
  localStorage.setItem(`oore_auth_last_method_${DEMO_INSTANCE_ID}`, 'oidc')
  localStorage.setItem(
    `oore_auth_last_at_${DEMO_INSTANCE_ID}`,
    String(Math.floor(Date.now() / 1000)),
  )
}

export async function enableDemoMode(): Promise<void> {
  seedDemoStores()

  const { setupWorker } = await import('msw/browser')
  const { allHandlers } = await import('./handlers')

  const worker = setupWorker(...allHandlers)

  await worker.start({
    onUnhandledRequest: 'warn',
    serviceWorker: {
      url: '/mockServiceWorker.js',
    },
  })
}
