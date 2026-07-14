import { BUILD_IDS } from '../seed'
import type { BuildLogChunk } from '@/lib/types'

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
    content: 'Signing APK with release keystore...',
    stream: 'stdout',
  },
  { sequence: 22, content: 'V2 signing enabled.', stream: 'stdout' },
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
  { sequence: 5, content: '$ flutter analyze', stream: 'stdout' },
  { sequence: 6, content: 'Analyzing native_payments...', stream: 'stdout' },
  { sequence: 7, content: '', stream: 'stdout' },
  {
    sequence: 8,
    content:
      "  error - The argument type 'String' can't be assigned to the parameter type 'PaymentIntent' - lib/stripe_v3/handler.dart:42:15 - argument_type_not_assignable",
    stream: 'stderr',
  },
  {
    sequence: 9,
    content:
      "  error - Undefined name 'StripeV3Client' - lib/stripe_v3/client.dart:18:5 - undefined_identifier",
    stream: 'stderr',
  },
  {
    sequence: 10,
    content:
      "  error - Missing required argument 'apiKey' - lib/stripe_v3/config.dart:7:22 - missing_required_argument",
    stream: 'stderr',
  },
  { sequence: 11, content: '', stream: 'stdout' },
  { sequence: 12, content: '3 issues found. (3 errors)', stream: 'stderr' },
  { sequence: 13, content: '', stream: 'stdout' },
  {
    sequence: 14,
    content: 'Build failed: flutter analyze exited with code 1',
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
    {
      sequence: sequence++,
      content: `[oore-step] ${JSON.stringify({ event: 'end', name: step.name, status: step.status })}`,
      stream: 'stdout',
    },
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

const failedBuildLogsWithSteps = withStepMarkers([
  {
    name: 'flutter pub get',
    command: 'flutter pub get',
    status: 'succeeded',
    logs: failedBuildLogs.slice(0, 5),
  },
  {
    name: 'flutter analyze',
    command: 'flutter analyze',
    status: 'failed',
    logs: failedBuildLogs.slice(5),
  },
])

export const demoBuildLogs: Record<string, Array<BuildLogChunk>> = {
  [BUILD_IDS.running1]: flutterBuildLogs.slice(0, 20), // in-progress
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
}
