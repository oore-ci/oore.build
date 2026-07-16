import RepositoryAvatar from '@/components/repository-avatar'
import type {
  Integration,
  IntegrationInstallation,
  IntegrationRepository,
} from '@/lib/types'
import { Badge } from '@/components/ui/badge'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'

function repositoryUrl(
  integration: Integration,
  repository: IntegrationRepository,
): string | null {
  if (integration.provider === 'local_git') return null
  return `${integration.host_url.replace(/\/$/, '')}/${repository.full_name}`
}

function RepositoryIdentity({
  integration,
  repository,
}: {
  integration: Integration
  repository: IntegrationRepository
}) {
  const content = (
    <>
      <RepositoryAvatar
        fullName={repository.full_name}
        avatarUrl={repository.avatar_url}
        repositoryId={repository.id}
        provider={integration.provider}
      />
      <span className="min-w-0 truncate font-medium">
        {repository.full_name}
      </span>
    </>
  )
  const url = repositoryUrl(integration, repository)

  return url ? (
    <a
      href={url}
      target="_blank"
      rel="noreferrer"
      className="group flex min-w-0 items-center gap-2 rounded-sm outline-none focus-visible:ring-2 focus-visible:ring-ring [&_span:last-child]:group-hover:underline"
    >
      {content}
    </a>
  ) : (
    <div className="flex min-w-0 items-center gap-2">{content}</div>
  )
}

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
      <section aria-labelledby="installations-title" className="min-w-0">
        <div className="flex items-center justify-between gap-4">
          <h2
            id="installations-title"
            className="text-sm font-medium uppercase tracking-wider text-muted-foreground"
          >
            {installationsLabel}
          </h2>
          <span className="text-xs text-muted-foreground">
            {installations.length} total
          </span>
        </div>

        {installations.length === 0 ? (
          <p className="mt-4 text-sm text-muted-foreground">
            No{' '}
            {integration.provider === 'gitlab'
              ? 'GitLab account'
              : 'installation'}{' '}
            yet
            {integration.provider === 'github' && integration.app_slug
              ? ' — install your GitHub App to get started.'
              : '.'}
          </p>
        ) : (
          <>
            <div className="mt-2 divide-y sm:hidden">
              {installations.map((installation) => (
                <article key={installation.id} className="space-y-2 py-4">
                  <div className="flex items-start justify-between gap-3">
                    <h3 className="min-w-0 truncate font-medium">
                      {installation.account_name}
                    </h3>
                    <Badge variant="outline">
                      {installation.account_type ?? 'Account'}
                    </Badge>
                  </div>
                  <p className="truncate font-mono text-xs text-muted-foreground">
                    {installation.external_id}
                  </p>
                </article>
              ))}
            </div>

            <div className="mt-2 hidden sm:block">
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>Account</TableHead>
                    <TableHead>Type</TableHead>
                    <TableHead className="hidden lg:table-cell">
                      External ID
                    </TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {installations.map((installation) => (
                    <TableRow key={installation.id}>
                      <TableCell className="font-medium">
                        {installation.account_name}
                      </TableCell>
                      <TableCell>
                        <Badge variant="outline">
                          {installation.account_type ?? 'Account'}
                        </Badge>
                      </TableCell>
                      <TableCell className="hidden font-mono text-xs text-muted-foreground lg:table-cell">
                        {installation.external_id}
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            </div>
          </>
        )}
      </section>

      <section aria-labelledby="repositories-title" className="min-w-0">
        <div className="flex items-center justify-between gap-4">
          <h2
            id="repositories-title"
            className="text-sm font-medium uppercase tracking-wider text-muted-foreground"
          >
            {repositoriesLabel}
          </h2>
          <span className="text-xs text-muted-foreground">
            {repositories.length} total
          </span>
        </div>

        {repositories.length === 0 ? (
          <p className="mt-4 text-sm text-muted-foreground">
            No{' '}
            {integration.provider === 'gitlab'
              ? 'GitLab projects'
              : 'repositories'}{' '}
            synced yet.
          </p>
        ) : (
          <>
            <div className="mt-2 divide-y sm:hidden">
              {repositories.map((repository) => (
                <article key={repository.id} className="space-y-2 py-4">
                  <RepositoryIdentity
                    integration={integration}
                    repository={repository}
                  />
                  <div className="flex items-center justify-between gap-3 pl-10">
                    <span className="truncate font-mono text-xs text-muted-foreground">
                      {repository.default_branch ?? 'Branch not set'}
                    </span>
                    <Badge
                      variant={repository.is_private ? 'secondary' : 'outline'}
                    >
                      {repository.is_private ? 'Private' : 'Public'}
                    </Badge>
                  </div>
                </article>
              ))}
            </div>

            <div className="mt-2 hidden sm:block">
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>Repository</TableHead>
                    <TableHead>Default branch</TableHead>
                    <TableHead className="hidden lg:table-cell">
                      Visibility
                    </TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {repositories.map((repository) => (
                    <TableRow key={repository.id}>
                      <TableCell>
                        <RepositoryIdentity
                          integration={integration}
                          repository={repository}
                        />
                      </TableCell>
                      <TableCell className="font-mono text-xs text-muted-foreground">
                        {repository.default_branch ?? '—'}
                      </TableCell>
                      <TableCell className="hidden lg:table-cell">
                        <Badge
                          variant={
                            repository.is_private ? 'secondary' : 'outline'
                          }
                        >
                          {repository.is_private ? 'Private' : 'Public'}
                        </Badge>
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            </div>
          </>
        )}
      </section>
    </>
  )
}
