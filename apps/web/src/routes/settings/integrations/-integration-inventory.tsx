import RepositoryAvatar from '@/components/repository-avatar'
import { toast } from '@/lib/toast'
import type {
  Integration,
  IntegrationInstallation,
  IntegrationRepository,
} from '@/lib/types'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger,
} from '@/components/ui/alert-dialog'
import { useUpdateRepositoryRunnerPolicy } from '@/hooks/use-integrations'
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

function RepositoryRunnerPolicy({
  canWrite,
  onChange,
  pending,
  repository,
}: {
  canWrite: boolean
  onChange: (allow: boolean) => void
  pending: boolean
  repository: IntegrationRepository
}) {
  const approved = repository.allow_direct_macos_runner
  return (
    <div className="flex items-center justify-end gap-2">
      <Badge variant={approved ? 'success' : 'outline'}>
        {approved ? 'Approved' : 'Needs approval'}
      </Badge>
      {canWrite ? (
        <AlertDialog>
          <AlertDialogTrigger
            render={
              <Button
                size="sm"
                variant="outline"
                disabled={pending}
                className="min-h-11 sm:min-h-8"
              >
                {pending ? 'Saving...' : approved ? 'Revoke' : 'Approve'}
              </Button>
            }
          />
          <AlertDialogContent>
            <AlertDialogHeader>
              <AlertDialogTitle>
                {approved
                  ? 'Revoke runner approval?'
                  : 'Approve this repository?'}
              </AlertDialogTitle>
              <AlertDialogDescription>
                {approved
                  ? 'Running builds will finish, but queued builds for every project linked to this repository will wait.'
                  : 'Build commands from every project linked to this repository will run with the macOS permissions of the runner account. Approve only code and contributors you would run directly on this Mac.'}
              </AlertDialogDescription>
            </AlertDialogHeader>
            <AlertDialogFooter>
              <AlertDialogCancel>Cancel</AlertDialogCancel>
              <AlertDialogAction onClick={() => onChange(!approved)}>
                {approved ? 'Revoke approval' : 'Approve repository'}
              </AlertDialogAction>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialog>
      ) : null}
    </div>
  )
}

export function IntegrationInventory({
  canWrite,
  installations,
  installationsLabel,
  integration,
  repositories,
  repositoriesLabel,
}: {
  canWrite: boolean
  installations: Array<IntegrationInstallation>
  installationsLabel: string
  integration: Integration
  repositories: Array<IntegrationRepository>
  repositoriesLabel: string
}) {
  const policyMutation = useUpdateRepositoryRunnerPolicy(integration.id)

  function updateRunnerPolicy(
    repository: IntegrationRepository,
    allow: boolean,
  ) {
    policyMutation.mutate(
      { repositoryId: repository.id, allow },
      {
        onSuccess: () =>
          toast.success(
            allow
              ? `${repository.full_name} approved for Direct runner builds.`
              : `${repository.full_name} runner approval revoked.`,
          ),
        onError: (error) =>
          toast.error(`Failed to update repository approval: ${error.message}`),
      },
    )
  }

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
                  <RepositoryRunnerPolicy
                    canWrite={canWrite}
                    onChange={(allow) => updateRunnerPolicy(repository, allow)}
                    pending={
                      policyMutation.isPending &&
                      policyMutation.variables.repositoryId === repository.id
                    }
                    repository={repository}
                  />
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
                    <TableHead className="text-right">Direct runner</TableHead>
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
                      <TableCell>
                        <RepositoryRunnerPolicy
                          canWrite={canWrite}
                          onChange={(allow) =>
                            updateRunnerPolicy(repository, allow)
                          }
                          pending={
                            policyMutation.isPending &&
                            policyMutation.variables.repositoryId ===
                              repository.id
                          }
                          repository={repository}
                        />
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
