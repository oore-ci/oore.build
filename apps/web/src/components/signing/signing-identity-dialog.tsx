import { useRef, useState } from 'react'
import { useForm, useFormState, useWatch } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import * as z from 'zod'
import { HugeiconsIcon } from '@hugeicons/react'
import { Add01Icon, Cancel01Icon, File01Icon } from '@hugeicons/core-free-icons'

import { isOoreApiError } from '@oore/client/client'
import type {
  AndroidKeystoreInput,
  IosDistributionInput,
  SigningIdentity,
  SigningIdentityKind,
} from '@oore/client/models'
import {
  useCreateSigningIdentity,
  useUpdateSigningIdentity,
} from '@/hooks/use-signing-identities'
import { getApiErrorMessage } from '@/lib/api-client/api-error'
import { fileToBase64 } from '@/lib/pipeline-form-utils'
import {
  SIGNING_IDENTITY_KIND_LABELS,
  SIGNING_IDENTITY_NAME,
  signingIdentitySummary,
  signingIdentityUseLabel,
} from '@/lib/signing-identities'
import { toast } from '@/lib/toast'
import { FileField } from '@/components/file-field'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
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
import { RadioGroup, RadioGroupItem } from '@/components/ui/radio-group'
import { Spinner } from '@/components/ui/spinner'
import { Mono } from '@/components/layout/mono'
import { Row } from '@/components/layout/row'
import { Stack } from '@/components/layout/stack'
import { Text } from '@/components/layout/text'

const identityFormSchema = z.object({
  name: z
    .string()
    .trim()
    .regex(
      SIGNING_IDENTITY_NAME,
      "Start with a letter or digit and use only letters, digits, '-' and '_' (at most 64).",
    ),
  kind: z.enum(['android_keystore', 'ios_distribution']),
  replace: z.boolean(),
  key_alias: z.string(),
  store_password: z.string(),
  key_password: z.string(),
  p12_password: z.string(),
})

type IdentityFormValues = z.infer<typeof identityFormSchema>

const IDENTITY_KINDS = [
  'ios_distribution',
  'android_keystore',
] as const satisfies ReadonlyArray<SigningIdentityKind>

const MATERIAL_SURFACE =
  'grid gap-3 rounded-[min(var(--radius-2xl),16px)] bg-muted/40 p-4'

function formValues(existing?: SigningIdentity): IdentityFormValues {
  return {
    name: existing?.name ?? '',
    kind: existing?.kind ?? 'ios_distribution',
    replace: !existing,
    key_alias: existing?.key_alias ?? '',
    store_password: '',
    key_password: '',
    p12_password: '',
  }
}

/** Several `.mobileprovision` files, one per bundle ID, chosen at once or one by one. */
function ProfileFilesField({
  error,
  files,
  onChange,
}: {
  error?: string
  files: Array<File>
  onChange: (files: Array<File>) => void
}) {
  const inputRef = useRef<HTMLInputElement>(null)
  const errorId = 'signing-profiles-error'
  return (
    <Stack gap={2}>
      <Label htmlFor="signing-profiles">Provisioning profiles</Label>
      {files.length > 0 ? (
        <ul className="grid gap-1.5">
          {files.map((file, index) => (
            <Row
              key={`${file.name}-${index}`}
              gap={2}
              render={<li />}
              className="rounded-xl bg-input/50 py-1 pr-1 pl-3 text-sm"
            >
              <HugeiconsIcon
                icon={File01Icon}
                className="size-4 shrink-0 text-muted-foreground"
                aria-hidden
              />
              <Mono className="min-w-0 flex-1 truncate">{file.name}</Mono>
              <Button
                type="button"
                variant="ghost"
                size="icon-xs"
                onClick={() => onChange(files.filter((_, i) => i !== index))}
              >
                <HugeiconsIcon icon={Cancel01Icon} />
                <span className="sr-only">Remove {file.name}</span>
              </Button>
            </Row>
          ))}
        </ul>
      ) : null}
      <div>
        <Button
          type="button"
          variant="outline"
          size="sm"
          aria-describedby={error ? errorId : undefined}
          onClick={() => inputRef.current?.click()}
        >
          <HugeiconsIcon icon={Add01Icon} />
          {files.length > 0 ? 'Add more profiles' : 'Choose profiles'}
        </Button>
      </div>
      <input
        ref={inputRef}
        id="signing-profiles"
        type="file"
        accept=".mobileprovision"
        multiple
        className="sr-only"
        tabIndex={-1}
        onChange={(event) => {
          const chosen = Array.from(event.target.files ?? [])
          event.target.value = ''
          if (chosen.length > 0) onChange([...files, ...chosen])
        }}
      />
      <Text variant="muted">
        One per bundle ID, all ad hoc or all App Store. Builds export the way
        the profiles say.
      </Text>
      {error ? (
        <p id={errorId} className="text-sm text-destructive">
          {error}
        </p>
      ) : null}
    </Stack>
  )
}

/**
 * Add a signing identity to the project's library, or rename or replace
 * `existing`. Key material is write-only: replacing it uploads the whole
 * keystore or certificate and profiles again.
 */
export function SigningIdentityDialog({
  existing,
  onOpenChange,
  open,
  projectId,
}: {
  existing?: SigningIdentity
  onOpenChange: (open: boolean) => void
  open: boolean
  projectId: string
}) {
  const createMutation = useCreateSigningIdentity(projectId)
  const updateMutation = useUpdateSigningIdentity(projectId)
  const mutation = existing ? updateMutation : createMutation
  const [keystore, setKeystore] = useState<File | null>(null)
  const [p12, setP12] = useState<File | null>(null)
  const [profiles, setProfiles] = useState<Array<File>>([])
  const [fileErrors, setFileErrors] = useState<
    Partial<Record<'keystore' | 'p12' | 'profiles', string>>
  >({})
  // Keep the edited identity's title while the dialog animates out.
  const [titleIdentity, setTitleIdentity] = useState(existing)
  if (open && existing !== titleIdentity) setTitleIdentity(existing)

  const form = useForm<IdentityFormValues>({
    resolver: zodResolver(identityFormSchema),
    values: formValues(existing),
  })
  const kind = useWatch({ control: form.control, name: 'kind' })
  const replace = useWatch({ control: form.control, name: 'replace' })
  const { errors } = useFormState({ control: form.control })

  function handleOpenChange(next: boolean) {
    // Closing mid-save would hide the result, including a save error.
    if (!next && mutation.isPending) return
    if (!next) {
      form.reset(formValues(existing))
      mutation.reset()
      setKeystore(null)
      setP12(null)
      setProfiles([])
      setFileErrors({})
    }
    onOpenChange(next)
  }

  function requireField(
    values: IdentityFormValues,
    name: 'key_alias' | 'store_password' | 'key_password',
    message: string,
  ) {
    if (values[name].trim()) return true
    form.setError(name, { message })
    return false
  }

  async function androidInput(
    values: IdentityFormValues,
  ): Promise<AndroidKeystoreInput | undefined> {
    const ok = [
      requireField(values, 'key_alias', 'Enter the key alias.'),
      requireField(values, 'store_password', 'Enter the keystore password.'),
      requireField(values, 'key_password', 'Enter the key password.'),
    ].every(Boolean)
    if (!keystore) setFileErrors({ keystore: 'Choose the keystore file.' })
    if (!ok || !keystore) return undefined
    return {
      keystore_filename: keystore.name,
      keystore_base64: await fileToBase64(keystore),
      key_alias: values.key_alias.trim(),
      store_password: values.store_password,
      key_password: values.key_password,
    }
  }

  async function iosInput(
    values: IdentityFormValues,
  ): Promise<IosDistributionInput | undefined> {
    const missing: typeof fileErrors = {}
    if (!p12) missing.p12 = 'Choose the distribution certificate (.p12).'
    if (profiles.length === 0) {
      missing.profiles = 'Add at least one provisioning profile.'
    }
    setFileErrors(missing)
    if (!p12 || profiles.length === 0) return undefined
    return {
      p12_filename: p12.name,
      p12_base64: await fileToBase64(p12),
      p12_password: values.p12_password,
      profiles: await Promise.all(
        profiles.map(async (file) => ({
          profile_filename: file.name,
          profile_base64: await fileToBase64(file),
        })),
      ),
    }
  }

  async function onSubmit(values: IdentityFormValues) {
    setFileErrors({})
    let android: AndroidKeystoreInput | undefined
    let ios: IosDistributionInput | undefined
    if (values.replace) {
      if (values.kind === 'android_keystore') {
        android = await androidInput(values)
        if (!android) return
      } else {
        ios = await iosInput(values)
        if (!ios) return
      }
    }
    const name = values.name.trim()
    const onSuccess = () => {
      toast.success(existing ? `Saved ${name}` : `Added ${name}`)
      handleOpenChange(false)
    }
    const onError = (err: Error) => {
      const message = getApiErrorMessage(err, {})
      if (isOoreApiError(err) && err.code === 'name_taken') {
        form.setError('name', { message })
        return
      }
      form.setError('root', { message })
    }
    if (existing) {
      updateMutation.mutate(
        {
          id: existing.id,
          name: name === existing.name ? undefined : name,
          android,
          ios,
        },
        { onSuccess, onError },
      )
    } else {
      createMutation.mutate(
        { name, kind: values.kind, android, ios },
        { onSuccess, onError },
      )
    }
  }

  const rootError = errors.root?.message
  const uses = existing?.used_by ?? []

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>
            {titleIdentity
              ? `Edit ${titleIdentity.name}`
              : 'Add signing identity'}
          </DialogTitle>
          <DialogDescription>
            {existing
              ? uses.length > 0
                ? `Picked by ${uses.map(signingIdentityUseLabel).join(', ')}. Their next build uses the change.`
                : 'Builds that name it pick up the change on their next run.'
              : 'Workflows name it under signing in .oore.yaml, or pick it in a pipeline’s Workflows settings.'}
          </DialogDescription>
        </DialogHeader>

        <Form {...form}>
          <form
            noValidate
            onSubmit={(event) => void form.handleSubmit(onSubmit)(event)}
            className="space-y-4"
          >
            {!existing ? (
              <FormField
                control={form.control}
                name="kind"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel>Kind</FormLabel>
                    <FormControl>
                      <RadioGroup
                        value={field.value}
                        onValueChange={(value) => {
                          const next = IDENTITY_KINDS.find(
                            (candidate) => candidate === value,
                          )
                          if (next) field.onChange(next)
                        }}
                        className="flex flex-wrap gap-x-6 gap-y-2"
                      >
                        {IDENTITY_KINDS.map((value) => (
                          <label
                            key={value}
                            className="flex items-center gap-2 text-sm"
                          >
                            <RadioGroupItem value={value} />
                            {SIGNING_IDENTITY_KIND_LABELS[value]}
                          </label>
                        ))}
                      </RadioGroup>
                    </FormControl>
                    <FormMessage />
                  </FormItem>
                )}
              />
            ) : null}

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
                      placeholder={
                        kind === 'android_keystore'
                          ? 'coin-upload'
                          : 'coin-app-store'
                      }
                      {...field}
                    />
                  </FormControl>
                  <FormDescription>
                    {existing
                      ? 'Files that name it under signing need the new name.'
                      : kind === 'android_keystore'
                        ? 'What a file writes under signing, such as android: coin-upload.'
                        : 'What a file writes under signing, such as ios: coin-app-store.'}
                  </FormDescription>
                  <FormMessage />
                </FormItem>
              )}
            />

            {existing ? (
              <Stack gap={2}>
                <Text variant="muted">
                  {SIGNING_IDENTITY_KIND_LABELS[existing.kind]} ·{' '}
                  {signingIdentitySummary(existing) || 'No details'}
                </Text>
                <FormField
                  control={form.control}
                  name="replace"
                  render={({ field }) => (
                    <label className="flex items-center gap-2 text-sm font-medium">
                      <Checkbox
                        checked={field.value}
                        onCheckedChange={(checked) => field.onChange(!!checked)}
                      />
                      {existing.kind === 'android_keystore'
                        ? 'Replace the keystore'
                        : 'Replace the certificate and profiles'}
                    </label>
                  )}
                />
              </Stack>
            ) : null}

            {replace && kind === 'android_keystore' ? (
              <div className={MATERIAL_SURFACE}>
                <FileField
                  id="signing-keystore"
                  label="Keystore (.jks)"
                  accept=".jks,.keystore"
                  file={keystore}
                  onFileChange={(next) => {
                    setKeystore(next)
                    setFileErrors({})
                  }}
                  pendingNote="uploads on save"
                  emptyLabel="No file chosen"
                  error={fileErrors.keystore}
                />
                <FormField
                  control={form.control}
                  name="key_alias"
                  render={({ field }) => (
                    <FormItem>
                      <FormLabel>Key alias</FormLabel>
                      <FormControl>
                        <Input
                          autoComplete="off"
                          placeholder="upload"
                          className="font-mono"
                          {...field}
                        />
                      </FormControl>
                      <FormMessage />
                    </FormItem>
                  )}
                />
                <div className="grid gap-3 sm:grid-cols-2">
                  <FormField
                    control={form.control}
                    name="store_password"
                    render={({ field }) => (
                      <FormItem>
                        <FormLabel>Keystore password</FormLabel>
                        <FormControl>
                          <Input
                            type="password"
                            autoComplete="new-password"
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
                    name="key_password"
                    render={({ field }) => (
                      <FormItem>
                        <FormLabel>Key password</FormLabel>
                        <FormControl>
                          <Input
                            type="password"
                            autoComplete="new-password"
                            className="font-mono"
                            {...field}
                          />
                        </FormControl>
                        <FormMessage />
                      </FormItem>
                    )}
                  />
                </div>
              </div>
            ) : null}

            {replace && kind === 'ios_distribution' ? (
              <div className={MATERIAL_SURFACE}>
                <FileField
                  id="signing-p12"
                  label="Distribution certificate (.p12)"
                  accept=".p12"
                  file={p12}
                  onFileChange={(next) => {
                    setP12(next)
                    setFileErrors({})
                  }}
                  pendingNote="uploads on save"
                  emptyLabel="No file chosen"
                  error={fileErrors.p12}
                />
                <FormField
                  control={form.control}
                  name="p12_password"
                  render={({ field }) => (
                    <FormItem>
                      <FormLabel>Certificate password</FormLabel>
                      <FormControl>
                        <Input
                          type="password"
                          autoComplete="new-password"
                          className="font-mono"
                          {...field}
                        />
                      </FormControl>
                      <FormDescription>
                        Leave empty if the .p12 has no password.
                      </FormDescription>
                      <FormMessage />
                    </FormItem>
                  )}
                />
                <ProfileFilesField
                  files={profiles}
                  onChange={(next) => {
                    setProfiles(next)
                    setFileErrors({})
                  }}
                  error={fileErrors.profiles}
                />
              </div>
            ) : null}

            {replace ? (
              <Text variant="caption">
                Oore stores the key material encrypted. Nobody can download it
                again.
              </Text>
            ) : null}

            {rootError ? (
              <p role="alert" className="text-sm text-destructive">
                {rootError}
              </p>
            ) : null}

            <DialogFooter>
              <Button
                type="button"
                variant="outline"
                onClick={() => handleOpenChange(false)}
                disabled={mutation.isPending}
              >
                Cancel
              </Button>
              <Button type="submit" disabled={mutation.isPending}>
                {mutation.isPending ? (
                  <Spinner data-icon="inline-start" />
                ) : null}
                {existing ? 'Save changes' : 'Add identity'}
              </Button>
            </DialogFooter>
          </form>
        </Form>
      </DialogContent>
    </Dialog>
  )
}
