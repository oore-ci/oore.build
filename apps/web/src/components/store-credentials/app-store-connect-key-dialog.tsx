import { useState } from 'react'
import { useForm } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import * as z from 'zod'

import { getApiErrorMessage } from '@/lib/api-client/api-error'
import { toast } from '@/lib/toast'
import {
  useCreateAppStoreConnectKey,
  useUpdateAppStoreConnectKey,
  type AppStoreConnectKeySummary,
} from '@/hooks/use-app-store-connect-keys'
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
import { Label } from '@/components/ui/label'
import { Spinner } from '@/components/ui/spinner'

const IDENTIFIER_PATTERN = /^[A-Za-z0-9-]+$/

const keyFormSchema = z.object({
  name: z
    .string()
    .trim()
    .min(1, 'Enter a name.')
    .max(64, 'Names can be at most 64 characters.'),
  key_id: z
    .string()
    .trim()
    .min(1, 'Enter the Key ID.')
    .regex(IDENTIFIER_PATTERN, 'Use the Key ID shown in App Store Connect.'),
  issuer_id: z
    .string()
    .trim()
    .min(1, 'Enter the Issuer ID.')
    .regex(IDENTIFIER_PATTERN, 'Use the Issuer ID shown in App Store Connect.'),
})

type KeyFormValues = z.infer<typeof keyFormSchema>

/**
 * Add a shared App Store Connect key, or rename or replace `existing`. The
 * private key is write-only, so editing keeps the stored one unless a new
 * `.p8` is chosen.
 */
export function AppStoreConnectKeyDialog({
  existing,
  onOpenChange,
  open,
}: {
  existing?: AppStoreConnectKeySummary
  onOpenChange: (open: boolean) => void
  open: boolean
}) {
  const createMutation = useCreateAppStoreConnectKey()
  const updateMutation = useUpdateAppStoreConnectKey()
  const mutation = existing ? updateMutation : createMutation
  const [file, setFile] = useState<File | null>(null)
  const [fileError, setFileError] = useState<string>()
  const form = useForm<KeyFormValues>({
    resolver: zodResolver(keyFormSchema),
    values: {
      name: existing?.name ?? '',
      key_id: existing?.key_id ?? '',
      issuer_id: existing?.issuer_id ?? '',
    },
  })

  function handleOpenChange(next: boolean) {
    if (!next) {
      form.reset()
      mutation.reset()
      setFile(null)
      setFileError(undefined)
    }
    onOpenChange(next)
  }

  async function onSubmit(values: KeyFormValues) {
    const keyIdChanged = existing && values.key_id !== existing.key_id
    if (!file && (!existing || keyIdChanged)) {
      setFileError(
        keyIdChanged
          ? 'Upload the .p8 file for the new Key ID.'
          : 'Upload the .p8 file.',
      )
      return
    }
    const privateKey = file ? btoa(await file.text()) : undefined
    const onSuccess = () => {
      toast.success(existing ? `Saved ${values.name}` : `Added ${values.name}`)
      handleOpenChange(false)
    }
    const onError = (err: Error) => {
      form.setError('root', {
        message: getApiErrorMessage(err, {
          key_exists: 'A shared key with this name already exists.',
        }),
      })
    }
    if (existing) {
      updateMutation.mutate(
        { id: existing.id, ...values, private_key_base64: privateKey },
        { onSuccess, onError },
      )
    } else {
      createMutation.mutate(
        { ...values, private_key_base64: privateKey ?? '' },
        { onSuccess, onError },
      )
    }
  }

  const inUse = existing?.pipeline_count ?? 0

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>
            {existing ? `Edit ${existing.name}` : 'Add App Store Connect key'}
          </DialogTitle>
          <DialogDescription>
            {existing
              ? inUse > 0
                ? `${inUse === 1 ? '1 pipeline uses' : `${inUse} pipelines use`} this key. Their next build picks up the change.`
                : 'No pipeline uses this key yet.'
              : 'Create a Team key in App Store Connect → Users and Access → Integrations, with App Manager access. Every pipeline can then pick it in iOS signing.'}
          </DialogDescription>
        </DialogHeader>

        <Form {...form}>
          <form
            onSubmit={(event) => void form.handleSubmit(onSubmit)(event)}
            className="space-y-4"
          >
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
                      placeholder="Acme team key"
                      {...field}
                    />
                  </FormControl>
                  <FormDescription>
                    Shown when choosing a key for a pipeline.
                  </FormDescription>
                  <FormMessage />
                </FormItem>
              )}
            />
            <div className="grid gap-4 sm:grid-cols-2">
              <FormField
                control={form.control}
                name="key_id"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel>Key ID</FormLabel>
                    <FormControl>
                      <Input
                        autoComplete="off"
                        spellCheck={false}
                        placeholder="ABC123XYZ9"
                        className="font-mono"
                        {...field}
                      />
                    </FormControl>
                    <FormMessage />
                  </FormItem>
                )}
              />
              <FormField
                control={form.control}
                name="issuer_id"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel>Issuer ID</FormLabel>
                    <FormControl>
                      <Input
                        autoComplete="off"
                        spellCheck={false}
                        placeholder="00000000-0000-0000-0000-000000000000"
                        className="font-mono"
                        {...field}
                      />
                    </FormControl>
                    <FormMessage />
                  </FormItem>
                )}
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="asc-key-file">Private key (.p8)</Label>
              <Input
                id="asc-key-file"
                type="file"
                accept=".p8,text/plain"
                aria-invalid={!!fileError}
                onChange={(event) => {
                  setFile(event.target.files?.[0] ?? null)
                  setFileError(undefined)
                }}
              />
              <p
                className={
                  fileError
                    ? 'text-xs text-destructive'
                    : 'text-xs text-muted-foreground'
                }
              >
                {fileError ??
                  (file
                    ? `Selected: ${file.name}`
                    : existing
                      ? 'Leave empty to keep the stored key.'
                      : 'Apple lets you download it once. Nobody can read it back from Oore.')}
              </p>
            </div>

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
                {existing ? 'Save changes' : 'Add key'}
              </Button>
            </DialogFooter>
          </form>
        </Form>
      </DialogContent>
    </Dialog>
  )
}
