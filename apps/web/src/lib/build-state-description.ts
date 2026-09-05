import type { Build, BuildEvent } from '@oore/client/models'

export function buildStateDescription(build: Build, events: Array<BuildEvent>) {
  if (build.runner_policy_block_reason || build.status === 'succeeded')
    return null

  const reason = [...events]
    .reverse()
    .find((event) => event.to_status === build.status && event.reason)?.reason
  const failedStep = build.step_results?.find(
    (step) => step.status === 'failed',
  )
  const descriptions = {
    queued:
      'Waiting for a runner assignment. No specific waiting reason has been reported.',
    scheduled: 'Scheduled for execution; the build has not started yet.',
    assigned: 'Assigned to a runner; waiting for execution to start.',
    running: 'Build in progress. Follow the current step in Logs.',
    failed: `Build failed${build.exit_code != null ? ` with exit code ${build.exit_code}` : ''}.${failedStep ? ` Failed step: ${failedStep.name}.` : ' No failed step was reported.'}`,
    timed_out: 'The build exceeded its time limit.',
    canceled: 'The build was canceled.',
    expired:
      'This build expired and will not execute. Run it again to create a new build.',
  }
  return reason || descriptions[build.status]
}
