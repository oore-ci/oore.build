import { useForm } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import z from 'zod'
import { HugeiconsIcon } from '@hugeicons/react'
import type { Instance } from '@/lib/types'
import { DEFAULT_INSTANCE_ICON_KEY, INSTANCE_ICONS } from '@/lib/instance-icons'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { useInstanceStore } from '@/stores/instance-store'

const editInstanceSchema = z.object({
  label: z.string().min(1, 'Label is required'),
  url: z
    .string()
    .transform((v) => v.replace(/\/+$/, ''))
    .pipe(
      z
        .string()
        .refine(
          (v) => v === '' || /^https?:\/\/.+/.test(v),
          'URL must be a valid HTTP/HTTPS URL, or empty for local dev',
        ),
    ),
  icon: z.string(),
})

type EditInstanceForm = z.infer<typeof editInstanceSchema>

interface EditInstanceDialogProps {
  instance: Instance | null
  open: boolean
  onOpenChange: (open: boolean) => void
}

export default function EditInstanceDialog({
  instance,
  open,
  onOpenChange,
}: EditInstanceDialogProps) {
  const updateInstance = useInstanceStore((s) => s.updateInstance)

  const {
    register,
    handleSubmit,
    formState: { errors, isValid },
    reset,
    watch,
    setValue,
  } = useForm<EditInstanceForm>({
    resolver: zodResolver(editInstanceSchema),
    defaultValues: {
      label: '',
      url: '',
      icon: DEFAULT_INSTANCE_ICON_KEY,
    },
    mode: 'onBlur',
  })

  const selectedIcon = watch('icon')

  function onSubmit(data: EditInstanceForm) {
    if (!instance) return
    updateInstance(instance.id, {
      label: data.label.trim(),
      url: data.url,
      icon: data.icon,
    })
    onOpenChange(false)
  }

  function handleOpenChange(nextOpen: boolean) {
    if (nextOpen && instance) {
      reset({
        label: instance.label,
        url: instance.url,
        icon: instance.icon ?? DEFAULT_INSTANCE_ICON_KEY,
      })
    } else if (!nextOpen) {
      reset()
    }
    onOpenChange(nextOpen)
  }

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Edit Instance</DialogTitle>
          <DialogDescription>
            Update the details for this backend instance.
          </DialogDescription>
        </DialogHeader>

        <form onSubmit={handleSubmit(onSubmit)} className="space-y-4">
          <div className="space-y-2">
            <Label htmlFor="edit-instance-label">Label</Label>
            <Input
              id="edit-instance-label"
              type="text"
              placeholder="My CI Server"
              {...register('label')}
              autoFocus
            />
            {errors.label ? (
              <p className="text-sm text-destructive">{errors.label.message}</p>
            ) : null}
          </div>

          <div className="space-y-2">
            <Label htmlFor="edit-instance-url">
              Backend URL{' '}
              <span className="text-muted-foreground font-normal">
                (optional)
              </span>
            </Label>
            <Input
              id="edit-instance-url"
              type="text"
              placeholder="https://ci.example.com"
              {...register('url')}
            />
            {errors.url ? (
              <p className="text-sm text-destructive">{errors.url.message}</p>
            ) : null}
          </div>

          <div className="space-y-2">
            <Label>Icon</Label>
            <div className="flex flex-wrap gap-2">
              {INSTANCE_ICONS.map((entry) => (
                <Button
                  key={entry.key}
                  type="button"
                  variant="outline"
                  size="icon"
                  className={
                    selectedIcon === entry.key
                      ? 'ring-2 ring-primary ring-offset-2 ring-offset-background'
                      : ''
                  }
                  onClick={() => setValue('icon', entry.key)}
                  title={entry.label}
                >
                  <HugeiconsIcon icon={entry.icon} size={16} />
                </Button>
              ))}
            </div>
          </div>

          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={() => handleOpenChange(false)}
            >
              Cancel
            </Button>
            <Button type="submit" disabled={!isValid}>
              Save
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}
