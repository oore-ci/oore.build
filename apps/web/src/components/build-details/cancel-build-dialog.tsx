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
          <AlertDialogTitle>Stop build #{buildNumber}?</AlertDialogTitle>
          <AlertDialogDescription>
            The runner stops the current step and the build ends as canceled.
            Artifacts it has not uploaded yet are lost. You can re-run it later.
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel disabled={isPending}>
            Keep running
          </AlertDialogCancel>
          <AlertDialogAction
            variant="destructive"
            disabled={isPending}
            onClick={onCancel}
          >
            {isPending ? 'Stopping...' : 'Stop build'}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}
