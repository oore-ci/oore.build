import { Link } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import { Download04Icon } from '@hugeicons/core-free-icons'
import type { Artifact } from '@oore/client/models'
import {
  artifactInstallReadiness,
  getIosAppMetadata,
} from '@/lib/artifact-install'
import { formatFileSize, relativeTime } from '@/lib/format-utils'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Row } from '@/components/layout/row'
import { Text } from '@/components/layout/text'

export function AppOutputs({
  artifacts,
  projectName,
}: {
  artifacts: Artifact[]
  projectName?: string | null
}) {
  const apps = artifacts
    .filter((artifact) => artifactInstallReadiness(artifact).ready)
    .slice(0, 6)
  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle>
          {apps.length ? 'Apps to install' : 'No apps to install'}
        </CardTitle>
      </CardHeader>
      <CardContent className="space-y-3">
        {apps.length ? (
          apps.map((artifact) => (
            <Row
              key={artifact.id}
              gap={3}
              className="flex-wrap justify-between rounded-lg border p-3"
            >
              <div className="min-w-0 flex-1 basis-48">
                <p className="font-medium">
                  {getIosAppMetadata(artifact)?.displayName ??
                    projectName ??
                    artifact.name}{' '}
                  <Badge variant="outline">
                    {artifact.artifact_type === 'apk' ? 'Android' : 'iOS'}
                  </Badge>
                </p>
                <Text variant="muted" className="mt-1 break-all">
                  {artifact.name}
                  {artifact.file_size != null
                    ? ` · ${formatFileSize(artifact.file_size)}`
                    : ''}
                  {` · Published ${relativeTime(artifact.created_at)}`}
                </Text>
              </div>
              <Button
                render={
                  <Link
                    to="/builds/$buildId"
                    params={{ buildId: artifact.build_id }}
                    search={{ install: artifact.id }}
                  />
                }
                nativeButton={false}
              >
                <HugeiconsIcon icon={Download04Icon} />
                Install
              </Button>
            </Row>
          ))
        ) : (
          <Text variant="muted">
            No installable app was found in recent outputs. Check output paths,
            file availability and iOS signing in the build details.
          </Text>
        )}
      </CardContent>
    </Card>
  )
}
