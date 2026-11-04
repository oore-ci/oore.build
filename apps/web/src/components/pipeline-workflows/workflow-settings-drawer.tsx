import { useState, type ReactNode } from 'react'
import { Link } from '@tanstack/react-router'
import { useForm, useFormState, useWatch } from 'react-hook-form'
import { HugeiconsIcon } from '@hugeicons/react'
import { Alert02Icon } from '@hugeicons/core-free-icons'

import type { BuildPlatform, SigningIdentity } from '@oore/client/models'
import { useIsMobile } from '@/hooks/use-mobile'
import { useUpdatePipelineWorkflowSettings } from '@/hooks/use-signing-identities'
import { getApiErrorMessage } from '@/lib/api-client/api-error'
import { BUILD_PLATFORM_LABELS } from '@/lib/build-platforms'
import {
  FOLLOW_FILE,
  IOS_EXPORT_METHOD_LABELS,
  workflowSettingsFormValues,
  workflowSettingsRequest,
  type ApprovalChoice,
  type WorkflowSettingsFormValues,
} from '@/lib/signing-identities'
import { toast } from '@/lib/toast'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import {
  Drawer,
  DrawerClose,
  DrawerContent,
  DrawerDescription,
  DrawerFooter,
  DrawerHeader,
  DrawerTitle,
} from '@/components/ui/drawer'
import {
  Form,
  FormControl,
  FormDescription,
  FormField,
  FormItem,
  FormLabel,
} from '@/components/ui/form'
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Spinner } from '@/components/ui/spinner'
import { Switch } from '@/components/ui/switch'
import { Mono } from '@/components/layout/mono'
import { Text } from '@/components/layout/text'
import {
  fileSigningName,
  signingPlatforms,
  type WorkflowRow,
} from '@/lib/pipeline-workflow-rows'

const APPROVAL_LABELS = {
  follow: 'Follow the pipeline’s approval policy',
  required: 'Every build waits for approval',
  not_required: 'Builds run without approval',
} satisfies Record<ApprovalChoice, string>

function identityLabel(identity: SigningIdentity) {
  return identity.export_method
    ? `${identity.name} (${IOS_EXPORT_METHOD_LABELS[identity.export_method]})`
    : identity.name
}

/** What the file plans for the workflow, so the choices below have context. */
function FileSummary({ row }: { row: WorkflowRow }) {
  if (!row.summary) {
    return (
      <Alert>
        <HugeiconsIcon icon={Alert02Icon} size={16} />
        <AlertDescription>
          The file no longer defines {row.name}. Its builds are refused, so
          these settings do nothing until it comes back. Clear them to free the
          identities they pick.
        </AlertDescription>
      </Alert>
    )
  }
  const rows: Array<[string, ReactNode]> = [
    [
      'Builds',
      row.summary.platforms.map((p) => BUILD_PLATFORM_LABELS[p]).join(', ') ||
        'Unknown',
    ],
    [
      'Publishes',
      row.summary.publishes.length > 0
        ? row.summary.publishes.map((line) => <div key={line}>{line}</div>)
        : 'Nothing',
    ],
  ]
  return (
    <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-xs">
      {rows.map(([label, value]) => (
        <div key={label} className="contents">
          <dt className="text-muted-foreground">{label}</dt>
          <dd className="min-w-0 wrap-break-word">{value}</dd>
        </div>
      ))}
    </dl>
  )
}

function IdentityField({
  disabled,
  fileName,
  form,
  identities,
  platform,
  projectId,
}: {
  disabled: boolean
  /** The identity the file names for this workflow, if any. */
  fileName?: string
  form: ReturnType<typeof useForm<WorkflowSettingsFormValues>>
  identities: Array<SigningIdentity>
  platform: BuildPlatform
  projectId: string
}) {
  const kind = platform === 'android' ? 'android_keystore' : 'ios_distribution'
  const name =
    platform === 'android' ? 'android_identity_id' : 'ios_identity_id'
  const choices = identities.filter((identity) => identity.kind === kind)
  return (
    <FormField
      control={form.control}
      name={name}
      render={({ field }) => {
        // Keep a picked identity visible even when the library lost it.
        const missing =
          field.value !== FOLLOW_FILE &&
          !choices.some((identity) => identity.id === field.value)
        const entries: Array<[string, string]> = [
          [
            FOLLOW_FILE,
            fileName
              ? `From the file: ${fileName}`
              : 'From the file, or the pipeline’s signing',
          ],
          ...choices.map((identity): [string, string] => [
            identity.id,
            identityLabel(identity),
          ]),
        ]
        if (missing) entries.push([field.value, 'Deleted identity'])
        const items = Object.fromEntries(entries)
        return (
          <FormItem>
            <FormLabel>{BUILD_PLATFORM_LABELS[platform]} signing</FormLabel>
            <Select
              value={field.value}
              onValueChange={(value) => field.onChange(value ?? FOLLOW_FILE)}
              items={items}
              disabled={disabled}
            >
              <FormControl>
                <SelectTrigger className="w-full">
                  <SelectValue />
                </SelectTrigger>
              </FormControl>
              <SelectContent>
                <SelectGroup>
                  {Object.entries(items).map(([value, label]) => (
                    <SelectItem key={value} value={value}>
                      {label}
                    </SelectItem>
                  ))}
                </SelectGroup>
              </SelectContent>
            </Select>
            <FormDescription>
              {missing ? (
                'This identity was deleted. Builds are refused until you pick another.'
              ) : choices.length === 0 ? (
                <>
                  The library has no{' '}
                  {platform === 'android'
                    ? 'Android keystore'
                    : 'iOS distribution'}{' '}
                  identity yet. Add one in{' '}
                  <Link
                    to="/projects/$projectId"
                    params={{ projectId }}
                    search={{ tab: 'settings' }}
                    hash="signing"
                    className="underline underline-offset-4"
                  >
                    Project settings &gt; Signing
                  </Link>
                  .
                </>
              ) : field.value === FOLLOW_FILE ? (
                `Uses the identity the file names under signing.${platform}, or the pipeline’s signing settings when it names none.`
              ) : (
                `Replaces the file’s signing.${platform} for this workflow.`
              )}
            </FormDescription>
          </FormItem>
        )
      }}
    />
  )
}

/** Uploads to App Store Connect with ad hoc profiles are refused when a build is created. */
function adHocUploadConflict(
  row: WorkflowRow,
  values: WorkflowSettingsFormValues,
  identities: Array<SigningIdentity>,
) {
  if (!values.publish_enabled) return false
  const identity = identities.find(
    (candidate) => candidate.id === values.ios_identity_id,
  )
  if (identity?.export_method !== 'ad_hoc') return false
  return (row.summary?.publishes ?? []).some((line) => line.startsWith('ios'))
}

/**
 * The Settings drawer for one workflow: the identities it signs with, whether
 * it publishes, and whether its builds wait for approval.
 */
export function WorkflowSettingsDrawer({
  canChangeApproval,
  canWrite,
  identities,
  identitiesError,
  onOpenChange,
  pipelineId,
  projectId,
  row,
}: {
  canChangeApproval: boolean
  canWrite: boolean
  identities: Array<SigningIdentity>
  identitiesError: Error | null
  onOpenChange: (open: boolean) => void
  pipelineId: string
  projectId: string
  row?: WorkflowRow
}) {
  const isMobile = useIsMobile()
  const mutation = useUpdatePipelineWorkflowSettings(pipelineId)
  // Keep the content while the drawer animates out after `row` clears.
  const [shown, setShown] = useState(row)
  if (row && row !== shown) setShown(row)
  const form = useForm<WorkflowSettingsFormValues>({
    values: workflowSettingsFormValues(shown?.settings),
  })
  const values = useWatch({ control: form.control })
  const { isDirty, errors } = useFormState({ control: form.control })

  function close(next: boolean) {
    if (!next && mutation.isPending) return
    if (!next) {
      form.reset()
      mutation.reset()
    }
    onOpenChange(next)
  }

  function onSubmit(next: WorkflowSettingsFormValues) {
    if (!shown) return
    mutation.mutate(
      { workflow: shown.name, ...workflowSettingsRequest(next) },
      {
        onSuccess: () => {
          toast.success(`Saved ${shown.name} settings`)
          close(false)
        },
        onError: (err) =>
          form.setError('root', { message: getApiErrorMessage(err, {}) }),
      },
    )
  }

  const disabled = !canWrite || mutation.isPending
  const conflict =
    shown &&
    adHocUploadConflict(
      shown,
      { ...workflowSettingsFormValues(), ...values },
      identities,
    )

  return (
    <Drawer
      open={row !== undefined}
      onOpenChange={close}
      showSwipeHandle={isMobile}
      swipeDirection={isMobile ? 'down' : 'right'}
    >
      <DrawerContent>
        <DrawerHeader>
          <DrawerTitle>
            <Mono>{shown?.name}</Mono> settings
          </DrawerTitle>
          <DrawerDescription>
            For this pipeline’s builds of the workflow. Anything left to the
            file follows its signing and publish settings.
          </DrawerDescription>
        </DrawerHeader>
        <Form {...form}>
          <form
            noValidate
            onSubmit={(event) => void form.handleSubmit(onSubmit)(event)}
            className="flex min-h-0 flex-1 flex-col"
          >
            <div className="flex-1 scroll-fade space-y-5 overflow-y-auto p-4">
              {shown ? <FileSummary row={shown} /> : null}

              {identitiesError ? (
                <Alert variant="destructive">
                  <HugeiconsIcon icon={Alert02Icon} size={16} />
                  <AlertDescription>
                    The signing library could not be loaded:{' '}
                    {identitiesError.message}
                  </AlertDescription>
                </Alert>
              ) : null}

              {shown
                ? signingPlatforms(shown).map((platform) => (
                    <IdentityField
                      key={platform}
                      disabled={disabled}
                      fileName={fileSigningName(shown, platform)}
                      form={form}
                      identities={identities}
                      platform={platform}
                      projectId={projectId}
                    />
                  ))
                : null}

              <FormField
                control={form.control}
                name="publish_enabled"
                render={({ field }) => (
                  <FormItem>
                    <label className="flex items-center justify-between gap-3 text-sm font-medium">
                      Publishing
                      <FormControl>
                        <Switch
                          checked={field.value}
                          onCheckedChange={field.onChange}
                          disabled={disabled}
                        />
                      </FormControl>
                    </label>
                    <FormDescription>
                      {field.value
                        ? 'Uploads what the file and the pipeline’s Publish settings say.'
                        : 'No store uploads for this workflow, whatever the file says.'}
                    </FormDescription>
                  </FormItem>
                )}
              />

              {conflict ? (
                <Alert variant="destructive">
                  <HugeiconsIcon icon={Alert02Icon} size={16} />
                  <AlertDescription>
                    This workflow uploads to App Store Connect, which needs App
                    Store profiles. Builds with this ad hoc identity are
                    refused.
                  </AlertDescription>
                </Alert>
              ) : null}

              <FormField
                control={form.control}
                name="approval"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel>Approval</FormLabel>
                    <Select
                      value={field.value}
                      onValueChange={(value) =>
                        field.onChange(value ?? 'follow')
                      }
                      items={APPROVAL_LABELS}
                      disabled={disabled || !canChangeApproval}
                    >
                      <FormControl>
                        <SelectTrigger className="w-full">
                          <SelectValue />
                        </SelectTrigger>
                      </FormControl>
                      <SelectContent>
                        <SelectGroup>
                          {Object.entries(APPROVAL_LABELS).map(
                            ([value, label]) => (
                              <SelectItem key={value} value={value}>
                                {label}
                              </SelectItem>
                            ),
                          )}
                        </SelectGroup>
                      </SelectContent>
                    </Select>
                    <FormDescription>
                      {canChangeApproval
                        ? 'Replaces the pipeline’s approval policy for this workflow.'
                        : 'Only project maintainers and instance admins can change which builds wait for approval.'}
                    </FormDescription>
                  </FormItem>
                )}
              />

              {!canWrite ? (
                <Text variant="muted">
                  You can view these settings. Changing them needs pipeline
                  write access in this project.
                </Text>
              ) : null}

              {errors.root?.message ? (
                <Alert variant="destructive" role="alert">
                  <HugeiconsIcon icon={Alert02Icon} size={16} />
                  <AlertDescription>{errors.root.message}</AlertDescription>
                </Alert>
              ) : null}
            </div>
            <DrawerFooter>
              <DrawerClose
                render={
                  <Button variant="outline" disabled={mutation.isPending}>
                    {canWrite ? 'Cancel' : 'Close'}
                  </Button>
                }
              />
              {canWrite ? (
                <Button type="submit" disabled={!isDirty || mutation.isPending}>
                  {mutation.isPending ? (
                    <Spinner data-icon="inline-start" />
                  ) : null}
                  Save changes
                </Button>
              ) : null}
            </DrawerFooter>
          </form>
        </Form>
      </DrawerContent>
    </Drawer>
  )
}
