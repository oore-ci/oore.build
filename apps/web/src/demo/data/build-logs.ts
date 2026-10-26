import { BUILD_IDS } from '../seed'
import type { BuildLogChunk } from '@oore/client/models'

const flutterBuildLogs: Array<BuildLogChunk> = [
  { sequence: 0, content: '$ flutter pub get', stream: 'stdout' },
  { sequence: 1, content: 'Resolving dependencies...', stream: 'stdout' },
  {
    sequence: 2,
    content: '  cached_network_image 3.3.1 (3.4.1 available)',
    stream: 'stdout',
  },
  { sequence: 3, content: '  dio 5.4.3+1', stream: 'stdout' },
  { sequence: 4, content: '  flutter_bloc 8.1.6', stream: 'stdout' },
  { sequence: 5, content: '  freezed_annotation 2.4.4', stream: 'stdout' },
  { sequence: 6, content: '  go_router 14.2.7', stream: 'stdout' },
  { sequence: 7, content: 'Got dependencies!', stream: 'stdout' },
  { sequence: 8, content: '', stream: 'stdout' },
  {
    sequence: 9,
    content: '$ flutter analyze --no-fatal-infos',
    stream: 'stdout',
  },
  { sequence: 10, content: 'Analyzing flutter_shop...', stream: 'stdout' },
  { sequence: 11, content: '', stream: 'stdout' },
  {
    sequence: 12,
    content:
      "  info - Unused import: 'package:flutter/foundation.dart' - lib/utils/logger.dart:3:8 - unused_import",
    stream: 'stdout',
  },
  { sequence: 13, content: '', stream: 'stdout' },
  { sequence: 14, content: '1 issue found. (1 info)', stream: 'stdout' },
  { sequence: 15, content: '', stream: 'stdout' },
  {
    sequence: 16,
    content: '$ flutter build apk --release --split-per-abi',
    stream: 'stdout',
  },
  {
    sequence: 17,
    content: "Running Gradle task 'assembleRelease'...",
    stream: 'stdout',
  },
  {
    sequence: 18,
    content: 'Note: Some input files use or override a deprecated API.',
    stream: 'stderr',
  },
  {
    sequence: 19,
    content: 'Note: Recompile with -Xlint:deprecation for details.',
    stream: 'stderr',
  },
  {
    sequence: 20,
    content: 'Note: Some input files use unchecked or unsafe operations.',
    stream: 'stderr',
  },
  {
    sequence: 21,
    content:
      'Font asset "MaterialIcons-Regular.otf" was tree-shaken, reducing it from 1645184 to 4076 bytes (99.8% reduction).',
    stream: 'stdout',
  },
  {
    sequence: 22,
    content: "Running Gradle task 'assembleRelease'... 41.3s",
    stream: 'stdout',
  },
  {
    sequence: 23,
    content:
      '✓ Built build/app/outputs/flutter-apk/app-armeabi-v7a-release.apk (18.2 MB)',
    stream: 'stdout',
  },
  {
    sequence: 24,
    content:
      '✓ Built build/app/outputs/flutter-apk/app-arm64-v8a-release.apk (19.1 MB)',
    stream: 'stdout',
  },
  {
    sequence: 25,
    content:
      '✓ Built build/app/outputs/flutter-apk/app-x86_64-release.apk (19.4 MB)',
    stream: 'stdout',
  },
  { sequence: 26, content: '', stream: 'stdout' },
  { sequence: 27, content: '$ flutter test --coverage', stream: 'stdout' },
  { sequence: 28, content: '00:03 +12: All tests passed!', stream: 'stdout' },
  { sequence: 29, content: 'Test coverage: 87.3%', stream: 'stdout' },
  { sequence: 30, content: '', stream: 'stdout' },
  { sequence: 31, content: 'Build completed successfully.', stream: 'stdout' },
]

const failedBuildLogs: Array<BuildLogChunk> = [
  { sequence: 0, content: '$ flutter pub get', stream: 'stdout' },
  { sequence: 1, content: 'Resolving dependencies...', stream: 'stdout' },
  { sequence: 2, content: '  stripe_sdk 3.0.0', stream: 'stdout' },
  { sequence: 3, content: 'Got dependencies!', stream: 'stdout' },
  { sequence: 4, content: '', stream: 'stdout' },
  { sequence: 5, content: '$ flutter test', stream: 'stdout' },
  {
    sequence: 6,
    content:
      '00:04 +38: test/stripe_v3/handler_test.dart: StripeV3Handler confirms a payment intent',
    stream: 'stdout',
  },
  {
    sequence: 7,
    content:
      '00:05 +38 -1: test/stripe_v3/handler_test.dart: StripeV3Handler rejects an expired card [E]',
    stream: 'stderr',
  },
  {
    sequence: 8,
    content: '  Expected: PaymentStatus:<PaymentStatus.declined>',
    stream: 'stderr',
  },
  {
    sequence: 9,
    content: '    Actual: PaymentStatus:<PaymentStatus.pending>',
    stream: 'stderr',
  },
  {
    sequence: 10,
    content:
      '00:09 +41 -2: test/checkout/checkout_flow_test.dart: Checkout retries after a network timeout [E]',
    stream: 'stderr',
  },
  {
    sequence: 11,
    content: '  TimeoutException after 0:00:05.000000: Future not completed',
    stream: 'stderr',
  },
  {
    sequence: 12,
    content: '00:11 +44 ~1 -2: Some tests failed.',
    stream: 'stderr',
  },
  {
    sequence: 13,
    content: '[oore-tests] 47 tests: 44 passed, 2 failed, 1 skipped',
    stream: 'stdout',
  },
  {
    sequence: 14,
    content: 'Build failed: flutter test exited with code 1',
    stream: 'stderr',
  },
]

function withStepMarkers(
  steps: Array<{
    name: string
    command: string
    status: string
    logs: Array<BuildLogChunk>
  }>,
): Array<BuildLogChunk> {
  let sequence = 0
  return steps.flatMap((step) => [
    {
      sequence: sequence++,
      content: `[oore-step] ${JSON.stringify({ event: 'start', name: step.name, command: step.command })}`,
      stream: 'stdout',
    },
    ...step.logs.map((log) => ({ ...log, sequence: sequence++ })),
    // A running step has no end marker yet.
    ...(step.status === 'running'
      ? []
      : [
          {
            sequence: sequence++,
            content: `[oore-step] ${JSON.stringify({ event: 'end', name: step.name, status: step.status })}`,
            stream: 'stdout' as const,
          },
        ]),
  ])
}

const succeededBuildLogsWithSteps = withStepMarkers([
  {
    name: 'flutter pub get',
    command: 'flutter pub get',
    status: 'succeeded',
    logs: flutterBuildLogs.slice(0, 9),
  },
  {
    name: 'flutter analyze',
    command: 'flutter analyze --no-fatal-infos',
    status: 'succeeded',
    logs: flutterBuildLogs.slice(9, 16),
  },
  {
    name: 'flutter build apk --release',
    command: 'flutter build apk --release --split-per-abi',
    status: 'succeeded',
    logs: flutterBuildLogs.slice(16, 27),
  },
  {
    name: 'flutter test --coverage',
    command: 'flutter test --coverage',
    status: 'succeeded',
    logs: flutterBuildLogs.slice(27),
  },
])

const runningBuildLogsWithSteps = withStepMarkers([
  {
    name: 'flutter pub get',
    command: 'flutter pub get',
    status: 'succeeded',
    logs: flutterBuildLogs.slice(0, 9),
  },
  {
    name: 'flutter analyze',
    command: 'flutter analyze --no-fatal-infos',
    status: 'succeeded',
    logs: flutterBuildLogs.slice(9, 16),
  },
  {
    name: 'flutter build apk --release',
    command: 'flutter build apk --release --split-per-abi',
    status: 'running',
    logs: flutterBuildLogs.slice(16, 20),
  },
])

const failedBuildLogsWithSteps = withStepMarkers([
  {
    name: 'flutter pub get',
    command: 'flutter pub get',
    status: 'succeeded',
    logs: failedBuildLogs.slice(0, 5),
  },
  {
    name: 'flutter test',
    command: 'flutter test',
    status: 'failed',
    logs: failedBuildLogs.slice(5),
  },
])

const failedPublishLogs = withStepMarkers([
  {
    name: 'checkout',
    command: 'git checkout b7c8d9e',
    status: 'succeeded',
    logs: [
      { sequence: 0, content: 'HEAD is now at b7c8d9e', stream: 'stdout' },
    ],
  },
  {
    name: 'flutter build appbundle --release',
    command: 'flutter build appbundle --release',
    status: 'succeeded',
    logs: [
      {
        sequence: 0,
        content: "Running Gradle task 'bundleRelease'...",
        stream: 'stdout',
      },
      {
        sequence: 1,
        content:
          '✓ Built build/app/outputs/bundle/release/app-release.aab (38.2 MB)',
        stream: 'stdout',
      },
    ],
  },
  {
    name: 'artifacts',
    command: 'Upload artifacts',
    status: 'succeeded',
    logs: [
      {
        sequence: 0,
        content: 'Found 1 artifact(s) to upload',
        stream: 'stdout',
      },
    ],
  },
  {
    name: 'publish-play',
    command: 'Upload the App Bundle to Google Play',
    status: 'failed',
    logs: [
      {
        sequence: 0,
        content:
          '[oore-publish] Google Play rejected the request (403): The caller does not have permission',
        stream: 'stderr',
      },
    ],
  },
])

export const demoBuildLogs = {
  [BUILD_IDS.failedPublish1]: failedPublishLogs,
  [BUILD_IDS.running1]: runningBuildLogsWithSteps,
  [BUILD_IDS.running2]: flutterBuildLogs.slice(0, 8),
  [BUILD_IDS.succeeded1]: succeededBuildLogsWithSteps,
  [BUILD_IDS.succeeded2]: flutterBuildLogs.slice(0, 8).concat([
    { sequence: 8, content: '$ flutter build apk --debug', stream: 'stdout' },
    {
      sequence: 9,
      content: "Running Gradle task 'assembleDebug'...",
      stream: 'stdout',
    },
    {
      sequence: 10,
      content: '✓ Built build/app/outputs/flutter-apk/app-debug.apk (42.6 MB)',
      stream: 'stdout',
    },
    {
      sequence: 11,
      content: 'Build completed successfully.',
      stream: 'stdout',
    },
  ]),
  [BUILD_IDS.failed1]: failedBuildLogsWithSteps,
  [BUILD_IDS.failed2]: [
    { sequence: 0, content: '$ flutter pub get', stream: 'stdout' },
    { sequence: 1, content: 'Got dependencies!', stream: 'stdout' },
    { sequence: 2, content: '$ flutter build apk --debug', stream: 'stdout' },
    {
      sequence: 3,
      content: "Running Gradle task 'assembleDebug'...",
      stream: 'stdout',
    },
    {
      sequence: 4,
      content: 'FAILURE: Build failed with an exception.',
      stream: 'stderr',
    },
    { sequence: 5, content: '* What went wrong:', stream: 'stderr' },
    {
      sequence: 6,
      content: "Execution failed for task ':app:compileDebugKotlin'.",
      stream: 'stderr',
    },
    {
      sequence: 7,
      content: '> Compilation error. See log for more details',
      stream: 'stderr',
    },
    {
      sequence: 8,
      content: 'Build failed: flutter build apk exited with code 2',
      stream: 'stderr',
    },
  ],
} satisfies Record<string, Array<BuildLogChunk>>
