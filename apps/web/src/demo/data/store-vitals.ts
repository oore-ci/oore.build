import type {
  StoreAppVitals,
  StoreVitalPoint,
  StoreVitalsResponse,
} from '@oore/client/models'
import { PROJECT_IDS, ago } from '../seed'

const DAY = 86_400

/** 28 daily rates drifting around `base`, oldest first. */
function dailyRates(base: number, wobble: number, seed: number) {
  return Array.from({ length: 28 }, (_, index): StoreVitalPoint => {
    const date = new Date((ago(0) - (28 - index) * DAY) * 1000)
    const wave =
      Math.sin((index + seed) * 0.9) + Math.sin((index + seed) * 0.37)
    return {
      label: date.toISOString().slice(0, 10),
      value: Math.max(0, base + wave * wobble),
    }
  })
}

function average(points: Array<StoreVitalPoint>) {
  const values = points.flatMap((point) =>
    point.value == null ? [] : [point.value],
  )
  return values.reduce((sum, value) => sum + value, 0) / values.length
}

function playVitals(
  crash: Array<StoreVitalPoint>,
  anr: Array<StoreVitalPoint>,
) {
  return [
    {
      kind: 'crash_rate' as const,
      value: average(crash),
      unit: 'ratio',
      threshold: 0.0109,
      points: crash,
    },
    {
      kind: 'anr_rate' as const,
      value: average(anr),
      unit: 'ratio',
      threshold: 0.0047,
      points: anr,
    },
  ]
}

const shopCrash = dailyRates(0.0042, 0.0008, 1)
const shopAnr = dailyRates(0.0037, 0.0005, 4)

const demoStoreApps: Array<Omit<StoreAppVitals, 'project_name'>> = [
  {
    project_id: PROJECT_IDS.flutterShop,
    store: 'google_play',
    app_identifier: 'build.oore.demo.shop',
    vitals: playVitals(shopCrash, shopAnr),
    issues: [
      {
        kind: 'crash',
        cause: 'java.lang.IllegalStateException',
        location:
          'io.flutter.embedding.engine.FlutterJNI.ensureAttachedToNative',
        reports: 1_284,
        users: 412,
        last_reported_at: ago(3_600),
        url: 'https://play.google.com/console',
      },
      {
        kind: 'anr',
        cause: 'Input dispatching timed out',
        location: 'com.acme.shop.checkout.PaymentSheet.onCreate',
        reports: 640,
        users: 388,
        last_reported_at: ago(7_200),
        url: 'https://play.google.com/console',
      },
      {
        kind: 'crash',
        cause: 'java.lang.OutOfMemoryError',
        location: 'android.graphics.BitmapFactory.nativeDecodeStream',
        reports: 211,
        users: 97,
        last_reported_at: ago(14_400),
        url: 'https://play.google.com/console',
      },
    ],
    refreshed_at: ago(2 * 3_600),
  },
  {
    project_id: PROJECT_IDS.flutterShop,
    store: 'app_store',
    app_identifier: 'build.oore.demo.shop',
    vitals: [
      {
        kind: 'hang_rate',
        value: 3.1,
        unit: 's/hr',
        points: [
          { label: '2.1.0', value: 5.6 },
          { label: '2.2.0', value: 4.9 },
          { label: '2.3.0', value: 4.2 },
          { label: '2.4.0', value: 3.1 },
        ],
      },
      {
        kind: 'launch_time',
        value: 640,
        unit: 'ms',
        points: [
          { label: '2.1.0', value: 910 },
          { label: '2.2.0', value: 820 },
          { label: '2.3.0', value: 700 },
          { label: '2.4.0', value: 640 },
        ],
      },
    ],
    issues: [],
    refreshed_at: ago(3 * 3_600),
  },
  {
    project_id: PROJECT_IDS.internalAdmin,
    store: 'google_play',
    app_identifier: 'build.oore.demo.admin',
    vitals: playVitals(
      dailyRates(0.0121, 0.0015, 2),
      dailyRates(0.0021, 0.0004, 7),
    ),
    issues: [
      {
        kind: 'crash',
        cause: 'kotlin.UninitializedPropertyAccessException',
        location: 'com.acme.admin.session.SessionStore.token',
        reports: 96,
        users: 41,
        last_reported_at: ago(1_800),
        url: 'https://play.google.com/console',
      },
    ],
    refreshed_at: ago(5 * 3_600),
  },
  {
    project_id: PROJECT_IDS.nativePayments,
    store: 'google_play',
    app_identifier: 'build.oore.demo.payments',
    vitals: [],
    issues: [],
    error:
      'Google Play refused the request: the service account needs View app information for this app.',
    refreshed_at: null,
  },
]

/** Mirrors `GET /v1/store-vitals` for the projects a persona can see. */
export function demoStoreVitals(
  projects: Array<{ id: string; name: string }>,
): StoreVitalsResponse {
  return {
    apps: demoStoreApps.flatMap((app) => {
      const project = projects.find((p) => p.id === app.project_id)
      return project ? [{ ...app, project_name: project.name }] : []
    }),
  }
}
