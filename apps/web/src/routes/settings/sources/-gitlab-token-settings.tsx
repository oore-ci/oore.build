import { zodResolver } from '@hookform/resolvers/zod'
import { useState } from 'react'
import { useForm } from 'react-hook-form'
import * as z from 'zod'

import { toast } from '@/lib/toast'
import type { GitLabCredentialStatusResponse } from '@oore/client/models'
import {
  useGitLabTokenStatus,
  useReplaceGitLabToken,
} from '@/hooks/use-integrations'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import {
  Form,
  FormControl,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
} from '@/components/ui/form'
import { Input } from '@/components/ui/input'

const replaceTokenSchema = z.object({
  access_token: z.string().trim().min(1, 'Access token is required'),
})

type ReplaceTokenForm = z.infer<typeof replaceTokenSchema>

function statusLabel(status: GitLabCredentialStatusResponse['status']) {
  if (status === 'valid') return 'Valid'
  if (status === 'expiring') return 'Expires soon'
  if (status === 'expired') return 'Expired'
  if (status === 'rejected') return 'Rejected by GitLab'
  return 'Could not check'
}

export function GitLabTokenSettings({
  canWrite,
  integrationId,
}: {
  canWrite: boolean
  integrationId: string
}) {
  const statusQuery = useGitLabTokenStatus(integrationId, true)
  const replaceMutation = useReplaceGitLabToken(integrationId)
  const [replaceOpen, setReplaceOpen] = useState(false)
  const form = useForm<ReplaceTokenForm>({
    resolver: zodResolver(replaceTokenSchema),
    defaultValues: { access_token: '' },
  })
  const status = statusQuery.data

  function handleOpenChange(open: boolean) {
    setReplaceOpen(open)
    if (!open) form.reset()
  }

  function handleReplace(values: ReplaceTokenForm) {
    replaceMutation.mutate(values, {
      onSuccess: () => {
        toast.success('GitLab access token replaced')
        handleOpenChange(false)
      },
      onError: (error) => {
        form.setError('access_token', { message: error.message })
      },
    })
  }

  return (
    <>
      <Card size="sm" aria-labelledby="gitlab-token-title">
        <CardHeader>
          <CardTitle id="gitlab-token-title">GitLab access token</CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          {statusQuery.error ? (
            <Alert variant="destructive">
              <AlertDescription>
                Oore could not check the saved token:{' '}
                {statusQuery.error.message}
              </AlertDescription>
            </Alert>
          ) : null}

          <div className="grid gap-3 text-sm sm:grid-cols-[12rem_1fr]">
            <span className="text-muted-foreground">Status</span>
            <div>
              {statusQuery.isLoading ? (
                <Badge variant="outline">Checking</Badge>
              ) : status ? (
                <Badge
                  variant={
                    status.status === 'valid'
                      ? 'success'
                      : status.status === 'unknown' ||
                          status.status === 'expiring'
                        ? 'outline'
                        : 'destructive'
                  }
                >
                  {statusLabel(status.status)}
                </Badge>
              ) : (
                <Badge variant="outline">Unknown</Badge>
              )}
            </div>

            <span className="text-muted-foreground">Expires</span>
            <span>
              {status?.expires_at
                ? new Date(status.expires_at * 1000).toLocaleString()
                : 'GitLab did not report an expiry date'}
            </span>

            <span className="text-muted-foreground">Last check</span>
            <span>
              {status?.checked_at
                ? new Date(status.checked_at * 1000).toLocaleString()
                : 'Not checked'}
            </span>
          </div>

          <div className="flex flex-wrap gap-2">
            <Button
              variant="outline"
              size="sm"
              disabled={statusQuery.isFetching}
              onClick={() => void statusQuery.refetch()}
            >
              {statusQuery.isFetching ? 'Checking...' : 'Check again'}
            </Button>
            {canWrite ? (
              <Button size="sm" onClick={() => setReplaceOpen(true)}>
                Replace access token
              </Button>
            ) : null}
          </div>
        </CardContent>
      </Card>

      <Dialog open={replaceOpen} onOpenChange={handleOpenChange}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Replace GitLab access token</DialogTitle>
            <DialogDescription>
              Oore checks the new token before it replaces the saved token.
              Existing projects and repositories stay connected.
            </DialogDescription>
          </DialogHeader>
          <Form {...form}>
            <form
              className="space-y-4"
              onSubmit={form.handleSubmit(handleReplace)}
            >
              <FormField
                control={form.control}
                name="access_token"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel>New access token</FormLabel>
                    <FormControl>
                      <Input
                        {...field}
                        type="password"
                        autoComplete="new-password"
                        placeholder="glpat-..."
                      />
                    </FormControl>
                    <FormMessage />
                  </FormItem>
                )}
              />
              <DialogFooter>
                <Button
                  type="button"
                  variant="outline"
                  onClick={() => handleOpenChange(false)}
                >
                  Cancel
                </Button>
                <Button type="submit" disabled={replaceMutation.isPending}>
                  {replaceMutation.isPending ? 'Replacing...' : 'Replace token'}
                </Button>
              </DialogFooter>
            </form>
          </Form>
        </DialogContent>
      </Dialog>
    </>
  )
}
