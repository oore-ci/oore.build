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
    { id: 'project', label: 'Create project', done: true },
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
        'Builds need a source. Choose the repository this project builds from.',
    }
  }
  if (!input.hasPipeline) {
    return {
      id: 'pipeline',
      title: 'Add a pipeline',
      detail:
        'A pipeline says what to build. Start from a Flutter starter or a checked-in Oore workflow.',
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
