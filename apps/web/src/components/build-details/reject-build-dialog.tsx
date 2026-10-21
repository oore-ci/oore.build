import { useState } from 'react'

import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog'
import { Label } from '@/components/ui/label'
import { Textarea } from '@/components/ui/textarea'

export default function RejectBuildDialog({
  buildNumber,
  isPending,
  onReject,
  onOpenChange,
  open,
}: {
  buildNumber: number
  isPending: boolean
  onReject: (reason: string) => void
  onOpenChange: (open: boolean) => void
  open: boolean
}) {
  const [reason, setReason] = useState('')

  return (
    <AlertDialog open={open} onOpenChange={onOpenChange}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Reject build #{buildNumber}?</AlertDialogTitle>
          <AlertDialogDescription>
            The build is canceled and never runs. The reason appears on its
            timeline and in the audit log.
          </AlertDialogDescription>
        </AlertDialogHeader>
        <div className="flex flex-col gap-2">
          <Label htmlFor="reject-build-reason">Reason (optional)</Label>
          <Textarea
            id="reject-build-reason"
            value={reason}
            maxLength={1000}
            onChange={(event) => setReason(event.target.value)}
          />
        </div>
        <AlertDialogFooter>
          <AlertDialogCancel disabled={isPending}>
            Keep waiting
          </AlertDialogCancel>
          <AlertDialogAction
            variant="destructive"
            disabled={isPending}
            onClick={() => onReject(reason)}
          >
            {isPending ? 'Rejecting...' : 'Reject build'}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}
