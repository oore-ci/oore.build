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

export default function CancelBuildDialog({
  buildNumber,
  isPending,
  onCancel,
  onOpenChange,
  open,
}: {
  buildNumber: number
  isPending: boolean
  onCancel: () => void
  onOpenChange: (open: boolean) => void
  open: boolean
}) {
  return (
    <AlertDialog open={open} onOpenChange={onOpenChange}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Cancel build #{buildNumber}?</AlertDialogTitle>
          <AlertDialogDescription>
            Running work will be stopped. Any incomplete artifacts may be
            unavailable.
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel disabled={isPending}>
            Keep running
          </AlertDialogCancel>
          <AlertDialogAction disabled={isPending} onClick={onCancel}>
            {isPending ? 'Canceling...' : 'Cancel build'}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}
