import { useState } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import { Copy01Icon, Refresh01Icon } from '@hugeicons/core-free-icons'

import { Alert, AlertDescription } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import { useRotateGitLabRepositoryWebhookSecret } from '@/hooks/use-integrations'
import { toast } from '@/lib/toast'
import type { IntegrationRepository } from '@/lib/types'

export function GitLabWebhookTokens({
  repositories,
}: {
  repositories: Array<IntegrationRepository>
}) {
  const rotate = useRotateGitLabRepositoryWebhookSecret()
  const [revealed, setRevealed] = useState<{
    repositoryId: string
    secret: string
  } | null>(null)

  function rotateToken(repository: IntegrationRepository) {
    rotate.mutate(repository.id, {
      onSuccess: (response) => {
        setRevealed({
          repositoryId: repository.id,
          secret: response.webhook_secret,
        })
        toast.success(`Webhook token rotated for ${repository.full_name}`)
      },
      onError: (error) =>
        toast.error(`Could not rotate webhook token: ${error.message}`),
    })
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-sm font-medium uppercase tracking-wider text-muted-foreground">
          GitLab webhook tokens
        </CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
        <p className="text-sm text-muted-foreground">
          Each project uses its own token. Generating a token immediately
          invalidates the previous token for that project.
        </p>
        {revealed ? (
          <Alert>
            <AlertDescription className="space-y-2">
              <p>
                Copy this token now. Oore stores it encrypted and will not show
                it again.
              </p>
              <div className="flex gap-2">
                <Input
                  readOnly
                  value={revealed.secret}
                  aria-label="New GitLab webhook token"
                  className="font-mono"
                />
                <Button
                  type="button"
                  variant="outline"
                  size="icon"
                  aria-label="Copy new GitLab webhook token"
                  onClick={() => {
                    void navigator.clipboard.writeText(revealed.secret).then(
                      () => toast.success('Webhook token copied'),
                      () => toast.error('Could not copy webhook token'),
                    )
                  }}
                >
                  <HugeiconsIcon icon={Copy01Icon} />
                </Button>
              </div>
            </AlertDescription>
          </Alert>
        ) : null}
        {repositories.length === 0 ? (
          <p className="text-sm text-muted-foreground">
            Sync GitLab projects before generating webhook tokens.
          </p>
        ) : (
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Project</TableHead>
                <TableHead className="w-44 text-right">Token</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {repositories.map((repository) => (
                <TableRow key={repository.id}>
                  <TableCell className="font-medium">
                    {repository.full_name}
                  </TableCell>
                  <TableCell className="text-right">
                    <Button
                      type="button"
                      variant="outline"
                      size="sm"
                      disabled={rotate.isPending}
                      onClick={() => rotateToken(repository)}
                    >
                      <HugeiconsIcon icon={Refresh01Icon} />
                      {rotate.isPending && rotate.variables === repository.id
                        ? 'Rotating...'
                        : 'Generate / rotate'}
                    </Button>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
      </CardContent>
    </Card>
  )
}
