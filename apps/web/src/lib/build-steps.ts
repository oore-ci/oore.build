/** Names for the build steps that can be retried on their own. */
const RETRY_STEP_LABELS = new Map([
  ['publish-play', 'Google Play upload'],
  ['publish-app-store', 'App Store upload'],
])

export function retryStepLabel(step: string): string {
  return RETRY_STEP_LABELS.get(step) ?? step
}
