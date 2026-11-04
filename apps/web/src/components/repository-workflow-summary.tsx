import { HugeiconsIcon } from '@hugeicons/react'
import { CheckmarkCircle02Icon } from '@hugeicons/core-free-icons'

import type { RepositoryWorkflowPreview } from '@oore/client/models'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Text } from '@/components/layout/text'

/** What a repository file builds, as read from one branch. */
export function RepositoryWorkflowSummary({
  workflow,
  reference,
}: {
  workflow: RepositoryWorkflowPreview
  reference: string
}) {
  const execution = workflow.execution
  if (!execution) return null
  const commandCount =
    execution.commands.pre_build.length +
    execution.commands.build.length +
    execution.commands.post_build.length +
    Object.values(execution.platform_commands).filter(Boolean).length

  return (
    <div className="space-y-4">
      <Alert>
        <HugeiconsIcon icon={CheckmarkCircle02Icon} size={16} />
        <AlertDescription>
          Oore will read <code>{workflow.path}</code> from the exact commit
          being built. This preview is from <code>{reference}</code>.
        </AlertDescription>
      </Alert>
      <div className="grid gap-3 text-sm sm:grid-cols-2">
        <div>
          <Text variant="caption">Platforms</Text>
          <p>{execution.platforms.join(', ')}</p>
        </div>
        <div>
          <Text variant="caption">Flutter</Text>
          <p>{execution.flutter_version ?? 'From .fvmrc, or stable'}</p>
        </div>
        <div>
          <Text variant="caption">Commands</Text>
          <p>
            {commandCount > 0
              ? `${commandCount} configured`
              : 'Platform defaults'}
          </p>
        </div>
        <div>
          <Text variant="caption">Artifacts</Text>
          <p className="font-mono text-xs">
            {execution.artifact_patterns.join(', ') || 'Platform defaults'}
          </p>
        </div>
      </div>
      {execution.env_keys.length > 0 ? (
        <Text variant="caption">
          Environment keys: {execution.env_keys.join(', ')}. Values stay hidden.
        </Text>
      ) : null}
    </div>
  )
}
