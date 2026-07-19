import type { Metric } from 'web-vitals'

import { resolveInstanceApiBaseUrl } from '@/lib/instance-url'
import type { UserRole } from '@/lib/types'
import { useAuthStore } from '@/stores/auth-store'
import { useInstanceStore } from '@/stores/instance-store'

type WebReleaseChannel = 'dev' | 'alpha' | 'beta' | 'stable'
type WebPersona =
  | 'operator_shell'
  | 'mobile_shell'
  | 'admin'
  | 'operator_build_detail'
  | 'qa_shell'
  | 'qa_install'
type WebPerformanceMetric =
  | 'lcp'
  | 'inp'
  | 'cls'
  | 'ttfb'
  | 'dom_content_loaded'
  | 'load'
  | 'render_error'
  | 'unhandled_rejection'

interface WebPerformanceObservation {
  metric: WebPerformanceMetric
  value: number
}

const pending: Array<WebPerformanceObservation> = []
let flushScheduled = false
let started = false

export function webReleaseChannel(
  value: string | undefined,
): WebReleaseChannel {
  return value === 'alpha' || value === 'beta' || value === 'stable'
    ? value
    : 'dev'
}

export function webPerformancePersona(
  pathname: string,
  role: UserRole,
  mobile: boolean,
  installView: boolean,
): WebPersona {
  const buildDetail = /^\/builds\/[^/]+\/?$/.test(pathname)
  if (buildDetail && (role === 'qa_viewer' || installView)) return 'qa_install'
  if (buildDetail) return 'operator_build_detail'
  if (pathname.startsWith('/settings')) return 'admin'
  if (mobile) return 'mobile_shell'
  return role === 'qa_viewer' ? 'qa_shell' : 'operator_shell'
}

function currentContext() {
  const { activeInstanceId, instances } = useInstanceStore.getState()
  const auth = useAuthStore.getState()
  if (
    !activeInstanceId ||
    auth.instanceId !== activeInstanceId ||
    !auth.token ||
    !auth.user
  )
    return null

  const baseUrl = resolveInstanceApiBaseUrl(instances[activeInstanceId])
  if (!baseUrl) return null

  return {
    baseUrl,
    token: auth.token,
    channel: webReleaseChannel(import.meta.env.VITE_RELEASE_CHANNEL),
    persona: webPerformancePersona(
      window.location.pathname,
      auth.user.role,
      window.matchMedia('(max-width: 767px)').matches,
      new URLSearchParams(window.location.search).has('install'),
    ),
  }
}

function flush() {
  flushScheduled = false
  const context = currentContext()
  if (!context) {
    pending.splice(0)
    return
  }
  const observations = pending.splice(0, 8)
  if (observations.length === 0) return

  void fetch(`${context.baseUrl}/v1/telemetry/web-performance`, {
    method: 'POST',
    headers: {
      Authorization: `Bearer ${context.token}`,
      'Content-Type': 'application/json',
    },
    body: JSON.stringify({
      channel: context.channel,
      persona: context.persona,
      observations,
    }),
    keepalive: true,
  }).catch(() => {
    // Best-effort operational telemetry must never affect the product path.
  })

  if (pending.length > 0) {
    flushScheduled = true
    queueMicrotask(flush)
  }
}

function queue(observation: WebPerformanceObservation) {
  if (!privacySignalsAllowMeasurement()) return
  if (!Number.isFinite(observation.value) || observation.value < 0) return
  pending.push(observation)
  if (flushScheduled) return
  flushScheduled = true
  queueMicrotask(flush)
}

export function reportWebRenderError() {
  queue({ metric: 'render_error', value: 1 })
}

export function reportWebUnhandledRejection() {
  queue({ metric: 'unhandled_rejection', value: 1 })
}

function reportVital(metric: Metric) {
  const names: Partial<Record<Metric['name'], WebPerformanceMetric>> = {
    CLS: 'cls',
    INP: 'inp',
    LCP: 'lcp',
    TTFB: 'ttfb',
  }
  const name = names[metric.name]
  if (name) queue({ metric: name, value: metric.value })
}

function reportNavigationTiming() {
  const navigation = performance.getEntriesByType('navigation')[0] as
    | PerformanceNavigationTiming
    | undefined
  if (!navigation) return
  queue({
    metric: 'dom_content_loaded',
    value: navigation.domContentLoadedEventEnd,
  })
  queue({ metric: 'load', value: navigation.loadEventEnd })
}

function privacySignalsAllowMeasurement() {
  const privacyNavigator = navigator as Navigator & {
    globalPrivacyControl?: boolean
  }
  return !privacyNavigator.globalPrivacyControl && navigator.doNotTrack !== '1'
}

export async function startWebPerformanceMonitoring() {
  if (started || !privacySignalsAllowMeasurement()) return
  started = true

  const { onCLS, onINP, onLCP, onTTFB } = await import('web-vitals')
  onCLS(reportVital)
  onINP(reportVital)
  onLCP(reportVital)
  onTTFB(reportVital)

  window.addEventListener('unhandledrejection', reportWebUnhandledRejection)

  if (document.readyState === 'complete') {
    reportNavigationTiming()
  } else {
    window.addEventListener('load', reportNavigationTiming, { once: true })
  }
}
