// Fixed demo constants — single source of truth for all mock data cross-references.

export const DEMO_INSTANCE_ID = 'demo-00000000-0000-0000-0000-000000000001'
// Resolved at runtime by enableDemoMode() via getDemoInstanceUrl().
// Must be a truthy string so `!!baseUrl` checks pass in query hooks.
// Demo mode intercepts fetch requests in-process before they hit the network.
export const DEMO_INSTANCE_URL = 'https://demo.oore.build'

/** Returns the real origin at runtime so MSW path matching works correctly. */
export function getDemoInstanceUrl(): string {
  return typeof window !== 'undefined'
    ? window.location.origin
    : DEMO_INSTANCE_URL
}
export const DEMO_INSTANCE_LABEL = 'Demo Instance'

export const DEMO_USER_ID = 'usr-demo-owner-001'
export const DEMO_USER_EMAIL = 'demo+owner@oore.build'

export const LEGACY_DEMO_AUTH_TOKEN = 'demo-session-token-static'
export const DEMO_PASSWORD = 'owner'
// Year 2099 — never expires during demo use
export const DEMO_AUTH_EXPIRES_AT = 4102444800

// ── Timestamps ────────────────────────────────────────────────────
const NOW = Math.floor(Date.now() / 1000)
export const ago = (seconds: number) => NOW - seconds

// ── Project IDs ───────────────────────────────────────────────────
export const PROJECT_IDS = {
  flutterShop: 'proj-demo-001',
  internalAdmin: 'proj-demo-002',
  nativePayments: 'proj-demo-003',
} as const

// ── Pipeline IDs ──────────────────────────────────────────────────
export const PIPELINE_IDS = {
  shopAndroid: 'pipe-demo-001',
  shopIos: 'pipe-demo-002',
  adminAndroid: 'pipe-demo-003',
  paymentsAll: 'pipe-demo-004',
} as const

// ── Build IDs ─────────────────────────────────────────────────────
export const BUILD_IDS = {
  running1: 'build-demo-001',
  running2: 'build-demo-002',
  queued1: 'build-demo-003',
  succeeded1: 'build-demo-004',
  succeeded2: 'build-demo-005',
  succeeded3: 'build-demo-006',
  failed1: 'build-demo-007',
  failed2: 'build-demo-008',
  canceled1: 'build-demo-009',
  timedOut1: 'build-demo-010',
  succeeded4: 'build-demo-011',
  succeeded5: 'build-demo-012',
  succeeded6: 'build-demo-013',
} as const

// ── Runner IDs ────────────────────────────────────────────────────
export const RUNNER_IDS = {
  macStudio: 'runner-demo-001',
  macMini: 'runner-demo-002',
} as const

// ── Integration IDs ───────────────────────────────────────────────
export const INTEGRATION_IDS = {
  github: 'integ-demo-gh-001',
  gitlab: 'integ-demo-gl-001',
} as const

// ── Notification Channel IDs ─────────────────────────────────────
export const NOTIFICATION_CHANNEL_IDS = {
  webhook: 'notif-demo-001',
  mattermost: 'notif-demo-002',
  email: 'notif-demo-003',
} as const

// ── User IDs ──────────────────────────────────────────────────────
export const USER_IDS = {
  owner: DEMO_USER_ID,
  admin: 'usr-demo-admin-002',
  developer: 'usr-demo-dev-003',
  qaViewer: 'usr-demo-qa-004',
  invited: 'usr-demo-invited-005',
} as const
