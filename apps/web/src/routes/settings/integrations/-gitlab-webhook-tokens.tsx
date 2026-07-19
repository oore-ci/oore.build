import { useState } from 'react'
import { DynamicLucideIcon } from '@/components/ui/dynamic-lucide-icon'
import { Copy as Copy01Icon, RefreshCw as Refresh01Icon } from 'lucide-react'

import {
  AlertDialog,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog'
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { useRotateGitLabRepositoryWebhookSecret } from '@/hooks/use-integrations'
import { toast } from '@/lib/toast'
import type { IntegrationRepository } from '@/lib/types'

interface RevealedWebhookToken {
  repository: IntegrationRepository
  secret: string
}

function copyToClipboard(value: string, label: string) {
  void navigator.clipboard.writeText(value).then(
    () => toast.success(`${label} copied`),
    () => toast.error(`Could not copy ${label.toLocaleLowerCase()}`),
  )
}

export function GitLabWebhookTokenDialogs({
  onClose,
  repository,
  webhookUrl,
}: {
  onClose: () => void
  repository: IntegrationRepository | null
  webhookUrl: string
}) {
  const rotate = useRotateGitLabRepositoryWebhookSecret()
  const [revealed, setRevealed] = useState<RevealedWebhookToken | null>(null)

  function generateToken() {
    if (!repository) return
    const target = repository
    rotate.mutate(target.id, {
      onSuccess: (response) => {
        setRevealed({
          repository: target,
          secret: response.webhook_secret,
        })
        onClose()
        toast.success(`Webhook token created for ${target.full_name}`)
      },
      onError: (error) =>
        toast.error(`Could not create webhook token: ${error.message}`),
    })
  }

  return (
    <>
      <AlertDialog
        open={repository !== null}
        onOpenChange={(open) => {
          if (!open && !rotate.isPending) onClose()
        }}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Create a webhook token?</AlertDialogTitle>
            <AlertDialogDescription>
              This creates a new token for {repository?.full_name}. If the
              project already has an Oore webhook token, it will stop working.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={rotate.isPending}>
              Cancel
            </AlertDialogCancel>
            <Button
              type="button"
              onClick={generateToken}
              disabled={rotate.isPending}
            >
              <DynamicLucideIcon icon={Refresh01Icon} />
              {rotate.isPending ? 'Creating...' : 'Create token'}
            </Button>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      <Dialog
        open={revealed !== null}
        onOpenChange={(open) => {
          if (!open) setRevealed(null)
        }}
      >
        <DialogContent showCloseButton={false}>
          <DialogHeader>
            <DialogTitle>
              Webhook token for {revealed?.repository.full_name}
            </DialogTitle>
            <DialogDescription>
              Add this URL and token to the project&apos;s GitLab webhook with
              Push events enabled. The token is shown once.
            </DialogDescription>
          </DialogHeader>

          <div className="space-y-4">
            <div className="space-y-2">
              <p className="text-xs font-medium text-muted-foreground">
                Webhook URL
              </p>
              <div className="flex gap-2">
                <Input
                  readOnly
                  value={webhookUrl}
                  aria-label="GitLab webhook URL"
                  className="font-mono text-xs"
                />
                <Button
                  type="button"
                  variant="outline"
                  size="icon"
                  aria-label="Copy GitLab webhook URL"
                  onClick={() => copyToClipboard(webhookUrl, 'Webhook URL')}
                >
                  <DynamicLucideIcon icon={Copy01Icon} />
                </Button>
              </div>
            </div>

            <div className="space-y-2">
              <p className="text-xs font-medium text-muted-foreground">
                Secret token
              </p>
              <div className="flex gap-2">
                <Input
                  readOnly
                  value={revealed?.secret ?? ''}
                  aria-label="New GitLab webhook token"
                  className="font-mono"
                />
                <Button
                  type="button"
                  variant="outline"
                  size="icon"
                  aria-label="Copy new GitLab webhook token"
                  onClick={() => {
                    if (revealed) {
                      copyToClipboard(revealed.secret, 'Webhook token')
                    }
                  }}
                >
                  <DynamicLucideIcon icon={Copy01Icon} />
                </Button>
              </div>
            </div>
          </div>

          <DialogFooter>
            <Button type="button" onClick={() => setRevealed(null)}>
              Done
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  )
}
