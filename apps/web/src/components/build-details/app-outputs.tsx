import { Link } from '@tanstack/react-router'
import type { Artifact } from '@oore/client/models'
import {
  artifactInstallReadiness,
  getIosAppMetadata,
} from '@/lib/artifact-install'
import { formatFileSize, relativeTime } from '@/lib/format-utils'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'

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
          {apps.length
            ? 'App outputs'
            : 'No install-ready app in recent outputs'}
        </CardTitle>
      </CardHeader>
      <CardContent className="space-y-3">
        {apps.length ? (
          apps.map((artifact) => (
            <div
              key={artifact.id}
              className="flex flex-wrap items-center justify-between gap-3 rounded-lg border p-3"
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
                <p className="mt-1 text-sm break-all text-muted-foreground">
                  {artifact.name}
                  {artifact.file_size != null
                    ? ` · ${formatFileSize(artifact.file_size)}`
                    : ''}
                  {` · Published ${relativeTime(artifact.created_at)}`}
                </p>
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
                Open install page
              </Button>
            </div>
          ))
        ) : (
          <p className="text-sm text-muted-foreground">
            A successful build can produce reports or files that cannot be
            installed. Check the build’s output paths, file availability and iOS
            signing in its details.
          </p>
        )}
      </CardContent>
    </Card>
  )
}
