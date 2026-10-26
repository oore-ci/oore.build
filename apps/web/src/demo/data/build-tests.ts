import { BUILD_IDS } from '../seed'
import type { BuildTestResultsResponse } from '@oore/client/models'

const HISTORY_WINDOW = 20

// Builds #42 and #43 predate the demo data set.
const older = [
  { build_id: 'build-demo-h42', build_number: 42 },
  { build_id: 'build-demo-h43', build_number: 43 },
]
const recent = [
  { build_id: BUILD_IDS.succeeded6, build_number: 44 },
  { build_id: BUILD_IDS.timedOut1, build_number: 45 },
  { build_id: BUILD_IDS.failed1, build_number: 46 },
]

function history(
  runs: Array<{ build_id: string; build_number: number }>,
  failed: Array<number>,
) {
  return runs.map((run) => ({
    ...run,
    status: failed.includes(run.build_number) ? 'failed' : 'passed',
  }))
}

const checkoutTimeout = {
  suite: 'test/checkout/checkout_flow_test.dart',
  name: 'Checkout retries after a network timeout',
  step: 'pre_build-3',
  message: 'TimeoutException after 0:00:05.000000: Future not completed',
  stack_trace:
    'dart:async  _CustomZone.run\npackage:fake_async/fake_async.dart 224:16  FakeAsync.flushTimers\ntest/checkout/checkout_flow_test.dart 61:11  main.<fn>.<fn>',
  duration_ms: 5012,
  flaky: true,
}

export const demoBuildTests = {
  [BUILD_IDS.succeeded6]: {
    summary: {
      total: 47,
      passed: 46,
      failed: 0,
      skipped: 1,
      duration_ms: 10400,
    },
    failures: [],
    history_window: HISTORY_WINDOW,
  },
  [BUILD_IDS.timedOut1]: {
    summary: {
      total: 47,
      passed: 45,
      failed: 1,
      skipped: 1,
      duration_ms: 15800,
    },
    failures: [
      {
        ...checkoutTimeout,
        history: history([...older, ...recent.slice(0, 2)], [42, 45]),
      },
    ],
    history_window: HISTORY_WINDOW,
  },
  [BUILD_IDS.failed1]: {
    summary: {
      total: 47,
      passed: 44,
      failed: 2,
      skipped: 1,
      duration_ms: 11200,
    },
    failures: [
      {
        ...checkoutTimeout,
        history: history([...older, ...recent], [42, 45, 46]),
      },
      {
        suite: 'test/stripe_v3/handler_test.dart',
        name: 'StripeV3Handler rejects an expired card',
        step: 'pre_build-3',
        message:
          'Expected: PaymentStatus:<PaymentStatus.declined>\n  Actual: PaymentStatus:<PaymentStatus.pending>',
        stack_trace:
          'package:matcher                         expect\ntest/stripe_v3/handler_test.dart 48:7  main.<fn>.<fn>',
        duration_ms: 84,
        flaky: false,
        history: history([...older, ...recent], [46]),
      },
    ],
    history_window: HISTORY_WINDOW,
  },
} satisfies Record<string, BuildTestResultsResponse>
