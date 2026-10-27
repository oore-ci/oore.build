/** Names for the build steps that can be retried on their own. */
const RETRY_STEP_LABELS = new Map([
  ['publish-play', 'Google Play upload'],
  ['publish-app-store', 'App Store upload'],
])

export function retryStepLabel(step: string): string {
  return RETRY_STEP_LABELS.get(step) ?? step
}

/** Labels for the steps the runner adds around a pipeline's own steps. */
const RUNNER_STEP_LABELS = new Map([
  ['checkout', 'Check out code'],
  ['vm-start', 'Start VM'],
  ['flutter-version', 'Set up Flutter'],
  ['android-sign', 'Verify Android signing'],
  ['ios-sign', 'Sign iOS app'],
  ['analyze', 'Analyze code'],
  ['artifacts', 'Collect artifacts'],
  ['publish-play', 'Upload to Google Play'],
  ['publish-app-store', 'Upload to App Store Connect'],
])

const STAGE_LABELS = new Map([
  ['pre_build', 'Pre-build'],
  ['build', 'Build'],
  ['post_build', 'Post-build'],
])

/** The name a v1 pipeline gives its unnamed commands, such as `pre_build-1`. */
const NUMBERED_STEP = /^(pre_build|build|post_build)-(\d+)$/

/**
 * The label to show for a build step. Runner steps and the numbered commands
 * of a v1 pipeline get readable labels; a step the pipeline file names keeps
 * its name. `stepNames` lists the build's steps, so a stage's command is only
 * numbered when the stage has more than one.
 */
export function buildStepLabel(
  name: string,
  stepNames: Iterable<string> = [],
): string {
  const runnerLabel = RUNNER_STEP_LABELS.get(name)
  if (runnerLabel) return runnerLabel

  const numbered = NUMBERED_STEP.exec(name)
  if (!numbered) return name
  const [, stage, index] = numbered
  const stageLabel = STAGE_LABELS.get(stage) ?? stage
  const hasSibling = [...stepNames].some(
    (other) => other !== name && NUMBERED_STEP.exec(other)?.[1] === stage,
  )
  return hasSibling || index !== '1' ? `${stageLabel} ${index}` : stageLabel
}
