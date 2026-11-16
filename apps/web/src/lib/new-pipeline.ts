import type {
  BuildPlatform,
  RepositoryWorkflowPreview,
} from '@oore/client/models'
import type { PipelineFormValues } from '@/lib/pipeline-schema'
import { BUILD_PLATFORM_LABELS } from '@/lib/build-platforms'
import {
  buildNumberOffsetField,
  defaultArtifactPatterns,
  parseMultiline,
  previewPlatformCommands,
  runsOnField,
  selectedPlatforms,
} from '@/lib/pipeline-form-utils'
import { EMPTY_PIPELINE_VALUES } from '@/lib/pipeline-templates'

/** "a", "a and b", "a, b and c". */
export function listWords(words: Array<string>): string {
  if (words.length <= 1) return words.join('')
  return `${words.slice(0, -1).join(', ')} and ${words.at(-1)}`
}

/**
 * A pipeline that builds from a repository file. Its build settings copy
 * the file's, so the pipeline still builds if the file is later removed.
 */
export function fileValues(
  workflow: RepositoryWorkflowPreview,
  suggestedName: string | undefined,
): PipelineFormValues {
  const execution = workflow.execution
  // `.oore.release.yaml` and `.oore/release.yaml` name a "Release"
  // pipeline; `.oore.yaml` and `ci/oore.yaml` say nothing about it.
  const workflowName = (workflow.path.split('/').at(-1) ?? '')
    .replace(/\.ya?ml$/, '')
    .replace(/^\.?oore(\.|$)/, '')
    .replaceAll('-', ' ')
    .trim()

  return {
    ...EMPTY_PIPELINE_VALUES,
    name:
      suggestedName ??
      (workflowName
        ? workflowName.replace(/\b\w/g, (letter) => letter.toUpperCase())
        : 'Repository workflow'),
    config_mode: 'explicit',
    config_path: workflow.path,
    platform_android: execution?.platforms.includes('android') ?? false,
    platform_ios: execution?.platforms.includes('ios') ?? false,
    platform_macos: execution?.platforms.includes('macos') ?? false,
    flutter_version: execution?.flutter_version ?? '',
    build_number_offset: buildNumberOffsetField(execution),
    runs_on: runsOnField(execution),
    enable_customization: true,
    pre_build_commands: execution?.commands.pre_build.join('\n') ?? '',
    build_commands: execution?.commands.build.join('\n') ?? '',
    post_build_commands: execution?.commands.post_build.join('\n') ?? '',
    android_build_args: execution?.platform_build_args.android.join('\n') ?? '',
    ios_build_args: execution?.platform_build_args.ios.join('\n') ?? '',
    macos_build_args: execution?.platform_build_args.macos.join('\n') ?? '',
    android_command_override: execution?.platform_commands.android ?? '',
    ios_command_override: execution?.platform_commands.ios ?? '',
    macos_command_override: execution?.platform_commands.macos ?? '',
    artifact_patterns: execution?.artifact_patterns.join('\n') ?? '',
    isolation: execution?.isolation ?? 'host',
    // A file with several workflows needs one picked, or builds fail.
    workflow: workflow.workflows.length > 1 ? workflow.workflows[0] : '',
    trigger_events: ['push'],
  }
}

/**
 * A pipeline without a repository file. An Android app gets a debug APK,
 * which needs no signing, so its first build can start at once; an app
 * without Android builds the platforms the repository has.
 */
export function defaultValues(
  detected: ReadonlyArray<BuildPlatform> | undefined,
): PipelineFormValues {
  const platforms = (detected ?? []).filter(
    (platform) => platform !== 'android',
  )
  if (!detected?.length || detected.includes('android')) {
    return {
      ...EMPTY_PIPELINE_VALUES,
      name: 'Debug APK',
      platform_android: true,
      android_command_override: 'flutter build apk --debug',
      artifact_patterns: 'build/app/outputs/flutter-apk/*.apk',
    }
  }
  return {
    ...EMPTY_PIPELINE_VALUES,
    name:
      platforms.length === 1
        ? `${BUILD_PLATFORM_LABELS[platforms[0]]} build`
        : 'App build',
    platform_android: false,
    platform_ios: platforms.includes('ios'),
    platform_macos: platforms.includes('macos'),
    artifact_patterns: defaultArtifactPatterns(platforms).join('\n'),
  }
}

/**
 * "Builds Android and iOS, 4 commands": what a new pipeline will run,
 * counted the way the planner lists its steps.
 */
export function planLine(values: PipelineFormValues): string {
  const platforms = selectedPlatforms(values).map(
    (platform) => BUILD_PLATFORM_LABELS[platform],
  )
  // The planner starts every build with `flutter pub get`.
  const commands =
    (platforms.length > 0 ? 1 : 0) +
    (values.enable_customization
      ? parseMultiline(values.pre_build_commands).length +
        parseMultiline(values.build_commands).length +
        parseMultiline(values.post_build_commands).length
      : 0) +
    previewPlatformCommands(values).length
  return `Builds ${platforms.length ? listWords(platforms) : 'nothing'}, ${commands} ${commands === 1 ? 'command' : 'commands'}`
}
