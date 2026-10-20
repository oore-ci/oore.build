import { useRef } from 'react'
import { useForm } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import * as z from 'zod'
import { HugeiconsIcon } from '@hugeicons/react'
import { FileUploadIcon } from '@hugeicons/core-free-icons'

import { getApiErrorMessage } from '@/lib/api-client/api-error'
import { toast } from '@/lib/toast'
import {
  SECRET_NAME_MAX_LENGTH,
  SECRET_NAME_PATTERN,
  SECRET_VALUE_MAX_BYTES,
  useCreateSecret,
  useRotateSecret,
  type SecretTarget,
} from '@/hooks/use-secrets'
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
  Form,
  FormControl,
  FormDescription,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
} from '@/components/ui/form'
import { Input } from '@/components/ui/input'
import { Spinner } from '@/components/ui/spinner'
import { Textarea } from '@/components/ui/textarea'

const valueSchema = z
  .string()
  .min(1, 'Enter a value.')
  .refine(
    (value) => new TextEncoder().encode(value).length <= SECRET_VALUE_MAX_BYTES,
    'Values can be at most 64 KiB.',
  )

const secretFormSchema = z.object({
  name: z
    .string()
    .trim()
    .min(1, 'Enter a name.')
    .max(SECRET_NAME_MAX_LENGTH, 'Names can be at most 128 characters.')
    .regex(
      SECRET_NAME_PATTERN,
      'Use letters, digits and underscores, not starting with a digit.',
    ),
  value: valueSchema,
})

type SecretFormValues = z.infer<typeof secretFormSchema>

const SCOPE_EFFECT = {
  instance: 'Every pipeline on this instance can use it.',
  project: 'Every pipeline in this project can use it.',
  pipeline: 'Only this pipeline can use it.',
} as const

/**
 * Create a secret, or replace the value of an existing one when `name` is set.
 * Values are write-only: the dialog never shows a stored value.
 */
export function SecretValueDialog({
  defaultName,
  name,
  onOpenChange,
  open,
  target,
}: {
  defaultName?: string
  name?: string
  onOpenChange: (open: boolean) => void
  open: boolean
  target: SecretTarget
}) {
  const isRotate = name !== undefined
  const createMutation = useCreateSecret(target)
  const rotateMutation = useRotateSecret(target)
  const mutation = isRotate ? rotateMutation : createMutation
  const fileInput = useRef<HTMLInputElement>(null)
  const form = useForm<SecretFormValues>({
    resolver: zodResolver(secretFormSchema),
    values: { name: name ?? defaultName ?? '', value: '' },
  })

  function handleOpenChange(next: boolean) {
    if (!next) {
      form.reset()
      mutation.reset()
    }
    onOpenChange(next)
  }

  async function loadFile(file: File | undefined) {
    if (!file) return
    form.setValue('value', await file.text(), {
      shouldDirty: true,
      shouldValidate: true,
    })
  }

  function onSubmit(values: SecretFormValues) {
    mutation.mutate(values, {
      onSuccess: () => {
        toast.success(
          isRotate
            ? `Replaced the value of ${values.name}`
            : `Added ${values.name}`,
        )
        handleOpenChange(false)
      },
      onError: (err) => {
        form.setError('root', {
          message: getApiErrorMessage(err, {
            secret_exists:
              'A secret with this name already exists here. Replace its value instead.',
          }),
        })
      },
    })
  }

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>
            {isRotate ? `Replace ${name}` : 'Add secret'}
          </DialogTitle>
          <DialogDescription>
            {isRotate
              ? 'The next build uses the new value. Builds already running keep the old one.'
              : `${SCOPE_EFFECT[target.scope]} Nobody can read the value back after you save it.`}
          </DialogDescription>
        </DialogHeader>

        <Form {...form}>
          <form
            onSubmit={(event) => void form.handleSubmit(onSubmit)(event)}
            className="space-y-4"
          >
            {isRotate ? null : (
              <FormField
                control={form.control}
                name="name"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel>Name</FormLabel>
                    <FormControl>
                      <Input
                        autoFocus
                        autoComplete="off"
                        spellCheck={false}
                        className="font-mono"
                        placeholder="PLAY_SERVICE_ACCOUNT"
                        {...field}
                      />
                    </FormControl>
                    <FormDescription>
                      Reference it as{' '}
                      <code className="font-mono">
                        {`\${{ secrets.${field.value.trim() || 'NAME'} }}`}
                      </code>
                    </FormDescription>
                    <FormMessage />
                  </FormItem>
                )}
              />
            )}

            <FormField
              control={form.control}
              name="value"
              render={({ field }) => (
                <FormItem>
                  <div className="flex items-center justify-between gap-2">
                    <FormLabel>Value</FormLabel>
                    <Button
                      type="button"
                      variant="ghost"
                      size="xs"
                      onClick={() => fileInput.current?.click()}
                    >
                      <HugeiconsIcon icon={FileUploadIcon} />
                      Load from file
                    </Button>
                    <input
                      ref={fileInput}
                      type="file"
                      className="sr-only"
                      tabIndex={-1}
                      aria-hidden
                      onChange={(event) => {
                        void loadFile(event.target.files?.[0])
                        event.target.value = ''
                      }}
                    />
                  </div>
                  <FormControl>
                    <Textarea
                      autoFocus={isRotate}
                      autoComplete="off"
                      spellCheck={false}
                      rows={5}
                      className="max-h-64 font-mono text-xs"
                      {...field}
                    />
                  </FormControl>
                  <FormDescription>
                    Up to 64 KiB. A Google Play service account JSON key fits.
                  </FormDescription>
                  <FormMessage />
                </FormItem>
              )}
            />

            {form.formState.errors.root ? (
              <p role="alert" className="text-sm text-destructive">
                {form.formState.errors.root.message}
              </p>
            ) : null}

            <DialogFooter>
              <Button
                type="button"
                variant="outline"
                onClick={() => handleOpenChange(false)}
              >
                Cancel
              </Button>
              <Button type="submit" disabled={mutation.isPending}>
                {mutation.isPending ? (
                  <Spinner data-icon="inline-start" />
                ) : null}
                {isRotate ? 'Replace value' : 'Add secret'}
              </Button>
            </DialogFooter>
          </form>
        </Form>
      </DialogContent>
    </Dialog>
  )
}
