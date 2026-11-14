import type {
  BuildPlatform,
  CreatePipelineRequest,
  ProjectAppDetection,
} from '@oore/client/models'
import type { PipelineFormValues } from '@/lib/pipeline-schema'
import { pipelineRequestFromForm } from '@/lib/pipeline-form-utils'
import { EMPTY_PIPELINE_VALUES } from '@/lib/pipeline-templates'

/** One pipeline the Add your app flow offers to create. */
export interface ProposedPipeline {
  key: string
  /** The platform the build needs a runner for; none for a config file. */
  platform?: BuildPlatform
  title: string
  detail: string
  /** Ticked when the flow opens. */
  selected: boolean
  /** Runs without signing, so the flow starts its first build. */
  runsNow: boolean
  request: CreatePipelineRequest
}

function fromForm(
  values: Partial<PipelineFormValues>,
  manualOnlyTriggers: boolean,
): CreatePipelineRequest {
  return pipelineRequestFromForm(
    { ...EMPTY_PIPELINE_VALUES, trigger_events: [], ...values },
    { canChangeApproval: false, manualOnlyTriggers },
  )
}

/**
 * The first pipelines for what the repository holds. A checked-in Oore file
 * wins, since it already says how to build; otherwise one starter per
 * platform folder, where only builds that need no signing start at once.
 */
export function proposedPipelines(
  detection: ProjectAppDetection,
  { manualOnlyTriggers }: { manualOnlyTriggers: boolean },
): Array<ProposedPipeline> {
  if (detection.config_files.length > 0) {
    return detection.config_files.map((path, index) => ({
      key: `file:${path}`,
      title: `Build from ${path}`,
      detail: 'Your repository already says how to build, so Oore uses it.',
      selected: index === 0,
      runsNow: true,
      request: {
        name:
          index === 0 ? 'Repository workflow' : `Repository workflow (${path})`,
        config_path: path,
        config_path_explicit: true,
      },
    }))
  }
  if (!detection.flutter) return []

  const proposals: Array<ProposedPipeline> = []
  const platforms = detection.platforms
  const only = platforms.length === 1
  if (platforms.includes('android')) {
    proposals.push({
      key: 'android',
      platform: 'android',
      title: 'Android test app',
      detail:
        'A debug APK anyone on the team can install. No signing needed, so the first build starts now.',
      selected: true,
      runsNow: true,
      request: fromForm(
        {
          name: 'Android test app',
          platform_android: true,
          android_command_override: 'flutter build apk --debug',
          artifact_patterns: 'build/app/outputs/flutter-apk/*.apk',
        },
        manualOnlyTriggers,
      ),
    })
  }
  if (platforms.includes('ios')) {
    proposals.push({
      key: 'ios',
      platform: 'ios',
      title: 'iOS test app',
      detail:
        'An ad hoc build for registered iPhones. It needs a certificate and profile, which you add next.',
      selected: only,
      runsNow: false,
      request: fromForm(
        {
          name: 'iOS test app',
          platform_android: false,
          platform_ios: true,
          artifact_patterns: 'build/ios/ipa/*.ipa',
        },
        manualOnlyTriggers,
      ),
    })
  }
  if (platforms.includes('macos')) {
    proposals.push({
      key: 'macos',
      platform: 'macos',
      title: 'macOS app',
      detail: 'A release build of the Mac app, unsigned.',
      selected: only,
      runsNow: true,
      request: fromForm(
        {
          name: 'macOS app',
          platform_android: false,
          platform_macos: true,
          artifact_patterns: 'build/macos/Build/Products/Release/*.app',
        },
        manualOnlyTriggers,
      ),
    })
  }
  return proposals
}
