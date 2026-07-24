import { useState } from 'react'
import { toast } from '@/lib/toast'
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
import {
  InputGroup,
  InputGroupAddon,
  InputGroupButton,
  InputGroupInput,
} from '@/components/ui/input-group'

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
  const CopyIcon = copied ? Tick02Icon : Copy01Icon

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
          <DialogTitle>Token created</DialogTitle>
          <DialogDescription>
            Make sure to copy your token now. You won&apos;t be able to see it
            again.
          </DialogDescription>
        </DialogHeader>
        <div className="space-y-3">
          <InputGroup>
            <InputGroupInput
              value={response?.token ?? ''}
              readOnly
              aria-label="Created API token"
              className="font-mono text-xs"
            />
            <InputGroupAddon align="inline-end">
              <InputGroupButton variant="ghost" size="xs" onClick={handleCopy}>
                <CopyIcon />
                {copied ? 'Copied' : 'Copy'}
              </InputGroupButton>
            </InputGroupAddon>
          </InputGroup>
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
