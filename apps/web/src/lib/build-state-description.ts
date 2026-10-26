import type { Build, BuildEvent } from '@oore/client/models'

export interface BuildStateSummary {
  /** Red only for failures the user has to fix; cancels and expiry are neutral. */
  tone: 'neutral' | 'destructive'
  title: string
  description?: string
}

function sentence(text: string) {
  const trimmed = text.trim()
  const capitalized = trimmed.charAt(0).toUpperCase() + trimmed.slice(1)
  return /[.!?]$/.test(capitalized) ? capitalized : `${capitalized}.`
}

export function buildStateSummary(
  build: Build,
  events: Array<BuildEvent>,
  { canApprove = false }: { canApprove?: boolean } = {},
): BuildStateSummary | null {
  if (build.runner_policy_block_reason || build.status === 'succeeded')
    return null
  if (build.approval?.state === 'pending') {
    return canApprove
      ? {
          tone: 'neutral',
          title: 'This build needs your approval',
          description:
            'No runner picks it up until you or another maintainer approve it.',
        }
      : {
          tone: 'neutral',
          title: 'Waiting for approval',
          description:
            'A project maintainer must approve this build before a runner picks it up.',
        }
  }

  const reason = [...events]
    .reverse()
    .find((event) => event.to_status === build.status && event.reason)?.reason
  const reasonSentence = reason ? sentence(reason) : undefined
  const failedStep = build.step_results?.find(
    (step) => step.status === 'failed' || step.status === 'timed_out',
  )

  switch (build.status) {
    case 'queued':
      return {
        tone: 'neutral',
        title: 'Waiting for a runner',
        description: reasonSentence,
      }
    case 'scheduled':
      return {
        tone: 'neutral',
        title: 'Scheduled',
        description: 'The build has not started yet.',
      }
    case 'assigned':
      return {
        tone: 'neutral',
        title: 'Assigned to a runner',
        description: 'Waiting for the runner to start it.',
      }
    case 'running':
      return null
    case 'failed':
      return {
        tone: 'destructive',
        title: failedStep ? `Failed at ${failedStep.name}` : 'Build failed',
        description:
          reasonSentence ??
          (build.exit_code != null
            ? `Exited with code ${build.exit_code}.`
            : undefined),
      }
    case 'timed_out':
      return {
        tone: 'destructive',
        title: failedStep
          ? `Timed out during ${failedStep.name}`
          : 'Build timed out',
        description: 'It ran past the pipeline time limit.',
      }
    case 'canceled':
      return {
        tone: 'neutral',
        title: 'Build canceled',
        description:
          reasonSentence ??
          (build.started_at
            ? 'It was stopped before it finished.'
            : 'It was canceled before it started.'),
      }
    case 'expired':
      return {
        tone: 'neutral',
        title: 'Build expired',
        description:
          'No runner picked it up in time, so it never ran and has no logs.',
      }
  }
}
