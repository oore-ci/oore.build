/**
 * The first-app setup checklist, as data. The project overview renders it in
 * full and Home renders the one-line summary; both read the same steps so the
 * two never disagree about where a project stands.
 */

export type SetupStepId =
  | 'repository'
  | 'project'
  | 'pipeline'
  | 'build'
  | 'install'

export interface SetupStep {
  done: boolean
  id: SetupStepId
  label: string
}

export interface SetupInput {
  artifactReady: boolean
  hasPipeline: boolean
  hasSource: boolean
  latestBuildStatus: string | undefined
}

export interface SetupNext {
  detail: string
  id: SetupStepId | 'done'
  title: string
}

export function setupSteps(input: SetupInput): Array<SetupStep> {
  return [
    { id: 'repository', label: 'Connect repository', done: input.hasSource },
    { id: 'project', label: 'Create app', done: true },
    { id: 'pipeline', label: 'Add pipeline', done: input.hasPipeline },
    {
      id: 'build',
      label: 'Run first build',
      done: input.latestBuildStatus !== undefined,
    },
    { id: 'install', label: 'Install the app', done: input.artifactReady },
  ]
}

export function setupDoneCount(input: SetupInput): number {
  return setupSteps(input).filter((step) => step.done).length
}

/** Setup ends when the first installable app exists. */
export function isSetupComplete(input: SetupInput): boolean {
  return input.artifactReady
}

export function setupNext(input: SetupInput): SetupNext {
  if (!input.hasSource) {
    return {
      id: 'repository',
      title: 'Connect a repository',
      detail:
        'Builds need a source. Choose the repository this app builds from.',
    }
  }
  if (!input.hasPipeline) {
    return {
      id: 'pipeline',
      title: 'Add a pipeline',
      detail:
        'Oore reads the repository and proposes what to build. Pick what you want, and the first build starts.',
    }
  }
  if (input.latestBuildStatus === undefined) {
    return {
      id: 'build',
      title: 'Run your first build',
      detail: 'Choose a pipeline and branch, then run the build.',
    }
  }
  if (input.artifactReady) {
    return {
      id: 'done',
      title: 'Your app is ready to install',
      detail: 'Install it on a device or share the link with your team.',
    }
  }
  if (input.latestBuildStatus === 'succeeded') {
    return {
      id: 'install',
      title: 'No app to install yet',
      detail:
        'The build succeeded but produced no installable app. Check output paths, file availability and iOS signing in the build details.',
    }
  }
  return {
    id: 'install',
    title: 'Follow your first build',
    detail: 'Watch progress and any errors. You can come back here later.',
  }
}

export type NextStepId = 'signing' | 'store' | 'testers'

export interface NextStep {
  action: string
  detail: string
  id: NextStepId
  title: string
}

/** The steps after the first install that the app has not done yet. */
export function nextSteps(progress: {
  release_signing: boolean
  store_publishing: boolean
  testers: boolean
}): Array<NextStep> {
  const steps: Array<{ step: NextStep; done: boolean }> = [
    {
      step: {
        id: 'signing',
        title: 'Sign a release',
        detail:
          'Add your keystore or certificate once, and release builds sign themselves.',
        action: 'Set up signing',
      },
      done: progress.release_signing,
    },
    {
      step: {
        id: 'store',
        title: 'Send to a store',
        detail:
          'Upload each build to Google Play or TestFlight, with the access checked first.',
        action: 'Set up a store',
      },
      done: progress.store_publishing,
    },
    {
      step: {
        id: 'testers',
        title: 'Invite a tester',
        detail:
          'Testers install each build from their phone and send feedback from it.',
        action: 'Invite',
      },
      done: progress.testers,
    },
  ]
  return steps.filter((entry) => !entry.done).map((entry) => entry.step)
}
