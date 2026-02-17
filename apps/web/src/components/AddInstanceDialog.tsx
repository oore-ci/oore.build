import { useForm } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import z from 'zod'
import { HugeiconsIcon } from '@hugeicons/react'
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
import {
  isHostedUiOrigin,
  isLocalLauncherOrigin,
  isLoopbackUrl,
} from '@/lib/connectivity'
import { useInstanceStore } from '@/stores/instance-store'
import { DEFAULT_INSTANCE_ICON_KEY, INSTANCE_ICONS } from '@/lib/instance-icons'

export function addInstanceSchema(frontendOrigin: string) {
  const hostedUi = isHostedUiOrigin(frontendOrigin)
  const localLauncher = isLocalLauncherOrigin(frontendOrigin)

  return z
    .object({
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
    .superRefine((values, ctx) => {
      if (hostedUi) {
        if (!values.url) {
          ctx.addIssue({
            code: z.ZodIssueCode.custom,
            path: ['url'],
            message:
              'Hosted UI requires an explicit HTTPS backend URL (or tunnel URL).',
          })
          return
        }

        if (!values.url.startsWith('http://')) return

        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['url'],
          message:
            'Hosted UI requires an HTTPS backend URL (use a tunnel or reverse proxy).',
        })
        return
      }

      if (!localLauncher || !values.url) return
      if (!isLoopbackUrl(values.url)) return

      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['url'],
        message:
          'When using oore-web locally, leave Backend URL empty to use the built-in proxy.',
      })
    })
}

type AddInstanceForm = z.infer<ReturnType<typeof addInstanceSchema>>

interface AddInstanceDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
}

export default function AddInstanceDialog({
  open,
  onOpenChange,
}: AddInstanceDialogProps) {
  const frontendOrigin =
    typeof window === 'undefined'
      ? 'http://localhost:3000'
      : window.location.origin
  const hostedUi = isHostedUiOrigin(frontendOrigin)
  const localLauncher = isLocalLauncherOrigin(frontendOrigin)
  const addInstance = useInstanceStore((s) => s.addInstance)
  const setActiveInstance = useInstanceStore((s) => s.setActiveInstance)

  const {
    register,
    handleSubmit,
    formState: { errors, isValid },
    reset,
    watch,
    setValue,
  } = useForm<AddInstanceForm>({
    resolver: zodResolver(addInstanceSchema(frontendOrigin)),
    defaultValues: { label: '', url: '', icon: DEFAULT_INSTANCE_ICON_KEY },
    mode: 'onBlur',
  })

  const selectedIcon = watch('icon')

  function onSubmit(data: AddInstanceForm) {
    const id = addInstance(data.label.trim(), data.url, data.icon)
    setActiveInstance(id)
    reset()
    onOpenChange(false)
  }

  function handleOpenChange(nextOpen: boolean) {
    if (!nextOpen) {
      reset()
    }
    onOpenChange(nextOpen)
  }

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Add Instance</DialogTitle>
          <DialogDescription>
            {hostedUi
              ? 'Connect to an HTTPS-reachable Oore backend (tunnel or reverse proxy).'
              : localLauncher
                ? 'Leave Backend URL empty to use the local oore-web proxy to your daemon.'
                : 'Connect to an Oore backend. Leave URL empty to use the local dev proxy.'}
          </DialogDescription>
        </DialogHeader>

        <form onSubmit={handleSubmit(onSubmit)} className="space-y-4">
          <div className="space-y-2">
            <Label htmlFor="instance-label">Label</Label>
            <Input
              id="instance-label"
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
            <Label htmlFor="instance-url">
              Backend URL{' '}
              {!hostedUi ? (
                <span className="text-muted-foreground font-normal">
                  (optional)
                </span>
              ) : null}
            </Label>
            <Input
              id="instance-url"
              type="text"
              placeholder="https://ci.example.com"
              {...register('url')}
            />
            {hostedUi ? (
              <p className="text-xs text-muted-foreground">
                `https://ci.oore.build` requires an explicit HTTPS backend URL
                and cannot connect to localhost `http://` backends directly.
              </p>
            ) : null}
            {localLauncher ? (
              <p className="text-xs text-muted-foreground">
                For local oore-web, keep this empty for localhost daemons to use
                the built-in proxy.
              </p>
            ) : null}
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
              Add
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}
