import { Link } from '@tanstack/react-router'

import { RepositoryFileLink } from '@/components/pipeline-page/source-label'

import { usePipelineWorkflows } from '@/hooks/use-pipelines'
import { describePublishLine } from '@/lib/pipeline-sources'
import { Text } from '@/components/layout/text'

/** What the pipeline's file says about uploads to one store. */
export interface FilePublishing {
  /** The file, such as `.oore/release.yaml`. */
  path: string
  /** Workflows whose file `publish` sends to this store, with what it says. */
  workflows: Array<{ name: string; line: string }>
  /** Every workflow that builds the platform sets `publish` for it. */
  all: boolean
}

/**
 * Reads the pipeline's file for `publish` on the platform's target. The file
 * beats the Publishing row, so the row says where it is set instead of
 * offering a choice that does nothing.
 */
export function useFilePublishing(
  pipelineId: string,
  platform: 'android' | 'ios',
): FilePublishing | null {
  const workflowsQuery = usePipelineWorkflows(pipelineId)
  const listing = workflowsQuery.data
  if (!listing || listing.state !== 'valid' || !listing.path) return null
  const building = listing.workflows.filter(
    (workflow) =>
      workflow.platforms.length === 0 || workflow.platforms.includes(platform),
  )
  const publishing = building.flatMap((workflow) => {
    const line = workflow.publishes.find((entry) =>
      entry.startsWith(`${platform} `),
    )
    return line
      ? [{ name: workflow.name, line: describePublishLine(line) }]
      : []
  })
  if (publishing.length === 0) return null
  return {
    path: listing.path,
    workflows: publishing,
    all: publishing.length === building.length,
  }
}

/** The read-only line when the file decides where builds go. */
export function FileControlledLine({ file }: { file: FilePublishing }) {
  const destinations = [...new Set(file.workflows.map((item) => item.line))]
  return (
    <div className="flex flex-col gap-1.5">
      <p className="text-sm">
        After a successful build, send to{' '}
        <span className="font-medium">{destinations.join(' or ')}</span>
      </p>
      <Text variant="muted" className="text-[13px]">
        from <RepositoryFileLink path={file.path} />
        {file.workflows.length > 1 || destinations.length > 1
          ? ` (${file.workflows.map((item) => item.name).join(', ')})`
          : null}
      </Text>
    </div>
  )
}

/** Under an editable line, when only some workflows set `publish`. */
export function PartlyFileControlledNote({ file }: { file: FilePublishing }) {
  return (
    <Text variant="muted" className="text-[13px]">
      {file.workflows.map((item, index) => (
        <span key={item.name}>
          {index > 0 ? ' ' : null}
          The <span className="font-mono">{item.name}</span> workflow sends to{' '}
          {item.line} instead.
        </span>
      ))}{' '}
      Set in <RepositoryFileLink path={file.path} />.
    </Text>
  )
}

/** Shown in place of the track list until the app's store is connected. */
export function NotConnectedLine({
  projectId,
  store,
}: {
  projectId: string
  store: 'Google Play' | 'App Store Connect'
}) {
  return (
    <p className="text-sm">
      Connect {store} in the app’s{' '}
      <Link
        to="/projects/$projectId"
        params={{ projectId }}
        search={{ tab: 'stores' }}
        className="underline underline-offset-4"
      >
        Stores
      </Link>{' '}
      to upload builds of this pipeline.
    </p>
  )
}
