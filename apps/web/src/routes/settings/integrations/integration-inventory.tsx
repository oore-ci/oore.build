import type {
  Integration,
  IntegrationInstallation,
  IntegrationRepository,
} from '@/lib/types'
import RepositoryAvatar from '@/components/repository-avatar'
import { Badge } from '@/components/ui/badge'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'

export function IntegrationInventory({
  installations,
  installationsLabel,
  integration,
  repositories,
  repositoriesLabel,
}: {
  installations: Array<IntegrationInstallation>
  installationsLabel: string
  integration: Integration
  repositories: Array<IntegrationRepository>
  repositoriesLabel: string
}) {
  return (
    <>
      <Card>
        <CardHeader>
          <CardTitle className="text-sm font-medium uppercase tracking-wider text-muted-foreground">
            {installationsLabel} ({installations.length})
          </CardTitle>
        </CardHeader>
        <CardContent>
          {installations.length === 0 ? (
            <p className="text-sm text-muted-foreground">
              No{' '}
              {integration.provider === 'gitlab'
                ? 'GitLab account'
                : 'installation'}{' '}
              yet
              {integration.provider === 'github' && integration.app_slug
                ? ' - install your GitHub App to get started.'
                : '.'}
            </p>
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Account</TableHead>
                  <TableHead>Type</TableHead>
                  <TableHead>External ID</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {installations.map((installation) => (
                  <TableRow key={installation.id}>
                    <TableCell>{installation.account_name}</TableCell>
                    <TableCell>{installation.account_type ?? '—'}</TableCell>
                    <TableCell className="font-mono text-xs text-muted-foreground">
                      {installation.external_id}
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="text-sm font-medium uppercase tracking-wider text-muted-foreground">
            {repositoriesLabel} ({repositories.length})
          </CardTitle>
        </CardHeader>
        <CardContent>
          {repositories.length === 0 ? (
            <p className="text-sm text-muted-foreground">
              No{' '}
              {integration.provider === 'gitlab'
                ? 'GitLab projects'
                : 'repositories'}{' '}
              synced yet.
            </p>
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Repository</TableHead>
                  <TableHead>Default branch</TableHead>
                  <TableHead>Visibility</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {repositories.map((repository) => (
                  <TableRow key={repository.id}>
                    <TableCell>
                      <div className="flex items-center gap-2">
                        <RepositoryAvatar
                          fullName={repository.full_name}
                          avatarUrl={repository.avatar_url}
                        />
                        <span>{repository.full_name}</span>
                      </div>
                    </TableCell>
                    <TableCell className="font-mono text-xs text-muted-foreground">
                      {repository.default_branch ?? '—'}
                    </TableCell>
                    <TableCell>
                      <Badge
                        variant={repository.is_private ? 'secondary' : 'outline'}
                      >
                        {repository.is_private ? 'private' : 'public'}
                      </Badge>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )}
        </CardContent>
      </Card>
    </>
  )
}
