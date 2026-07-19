import { useState } from 'react'
import { toast } from '@/lib/toast'
import { DynamicLucideIcon } from '@/components/ui/dynamic-lucide-icon'
import { Copy as Copy01Icon, Check as Tick02Icon } from 'lucide-react'

import type { CreateApiTokenResponse } from '@/lib/types'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'

interface TokenCreatedDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  response: CreateApiTokenResponse | null
}

export default function TokenCreatedDialog({
  open,
  onOpenChange,
  response,
}: TokenCreatedDialogProps) {
  const [copied, setCopied] = useState(false)

  function handleCopy() {
    if (!response) return
    void navigator.clipboard.writeText(response.token).then(() => {
      setCopied(true)
      toast.success('Token copied to clipboard')
      setTimeout(() => setCopied(false), 2000)
    })
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Token Created</DialogTitle>
          <DialogDescription>
            Make sure to copy your token now. You won&apos;t be able to see it
            again.
          </DialogDescription>
        </DialogHeader>
        <div className="space-y-3">
          <div className="flex items-center gap-2">
            <code className="flex-1 break-all rounded-md bg-muted px-3 py-2 font-mono text-sm">
              {response?.token}
            </code>
            <Button variant="outline" size="sm" onClick={handleCopy}>
              <DynamicLucideIcon icon={copied ? Tick02Icon : Copy01Icon} />
              {copied ? 'Copied' : 'Copy'}
            </Button>
          </div>
          <Alert>
            <AlertDescription>
              This token will not be shown again. Store it in a secure location.
            </AlertDescription>
          </Alert>
        </div>
        <DialogFooter>
          <Button onClick={() => onOpenChange(false)}>Done</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
