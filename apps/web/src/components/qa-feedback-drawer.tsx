import { useState, type ReactElement } from 'react'
import { useForm } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import * as z from 'zod'
import { HugeiconsIcon } from '@hugeicons/react'
import { Cancel01Icon } from '@hugeicons/core-free-icons'

import { useCreateBuildFeedback } from '@/hooks/use-build-feedback'
import { useIsMobile } from '@/hooks/use-mobile'
import { useObjectUrl } from '@/hooks/use-object-url'
import { getApiErrorMessage } from '@/lib/api-client/api-error'
import {
  FEEDBACK_MAX_FIELD_CHARS,
  FEEDBACK_MAX_MESSAGE_CHARS,
  FEEDBACK_SCREENSHOT_TYPES,
  detectFeedbackDevice,
  readFileAsBase64,
  screenshotProblem,
} from '@/lib/feedback-device'
import { formatFileSize } from '@/lib/format-utils'
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
  DrawerTrigger,
} from '@/components/ui/drawer'
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

const fieldSchema = z
  .string()
  .trim()
  .max(
    FEEDBACK_MAX_FIELD_CHARS,
    `Use ${FEEDBACK_MAX_FIELD_CHARS} characters or fewer`,
  )

const feedbackSchema = z.object({
  message: z
    .string()
    .trim()
    .min(1, 'Describe what happened')
    .max(
      FEEDBACK_MAX_MESSAGE_CHARS,
      `Use ${FEEDBACK_MAX_MESSAGE_CHARS} characters or fewer`,
    ),
  device: fieldSchema,
  os_version: fieldSchema,
  app_version: fieldSchema,
})

type FeedbackValues = z.infer<typeof feedbackSchema>

const FEEDBACK_ERRORS = {
  storage_unavailable:
    'Screenshots are not set up on this instance. Remove the screenshot and send the note on its own.',
}

function optional(value: string) {
  return value.trim() ? value.trim() : null
}

function ScreenshotField({
  file,
  error,
  onChange,
}: {
  file: File | null
  error: string | null
  onChange: (file: File | null) => void
}) {
  const previewUrl = useObjectUrl(file ?? undefined)

  return (
    <div className="space-y-2">
      <label htmlFor="feedback-screenshot" className="text-sm font-medium">
        Screenshot <span className="text-muted-foreground">(optional)</span>
      </label>
      {file && previewUrl ? (
        <div className="flex items-center gap-3 rounded-2xl border p-2">
          <img
            src={previewUrl}
            alt="Selected screenshot"
            className="size-14 shrink-0 rounded-lg border object-cover"
          />
          <div className="min-w-0 flex-1 text-sm">
            <p className="truncate font-medium">{file.name}</p>
            <p className="text-muted-foreground">{formatFileSize(file.size)}</p>
          </div>
          <Button
            type="button"
            variant="ghost"
            size="icon"
            aria-label="Remove screenshot"
            onClick={() => onChange(null)}
          >
            <HugeiconsIcon icon={Cancel01Icon} />
          </Button>
        </div>
      ) : (
        <Input
          id="feedback-screenshot"
          type="file"
          accept={FEEDBACK_SCREENSHOT_TYPES.join(',')}
          aria-invalid={error ? true : undefined}
          onChange={(event) => onChange(event.currentTarget.files?.[0] ?? null)}
        />
      )}
      {error ? (
        <p className="text-sm text-destructive">{error}</p>
      ) : (
        <p className="text-sm text-muted-foreground">
          PNG, JPEG or WebP, up to 5 MB.
        </p>
      )}
    </div>
  )
}

/**
 * One-tap feedback for testers: files a note against the build with the
 * device, OS and app version filled in.
 */
export function QaFeedbackDrawer({
  appVersion,
  artifactId,
  buildId,
  children,
}: {
  appVersion: string
  artifactId?: string
  buildId: string
  children: ReactElement
}) {
  const isMobile = useIsMobile()
  const [open, setOpen] = useState(false)
  const [screenshot, setScreenshot] = useState<File | null>(null)
  const [screenshotError, setScreenshotError] = useState<string | null>(null)
  const mutation = useCreateBuildFeedback(buildId)
  const form = useForm<FeedbackValues>({
    resolver: zodResolver(feedbackSchema),
    defaultValues: {
      message: '',
      device: '',
      os_version: '',
      app_version: appVersion,
    },
  })

  function prefillDevice() {
    void detectFeedbackDevice(globalThis.navigator).then((detected) => {
      if (!form.getFieldState('device').isDirty)
        form.setValue('device', detected.device)
      if (!form.getFieldState('os_version').isDirty)
        form.setValue('os_version', detected.osVersion)
    })
  }

  function reset() {
    form.reset({
      message: '',
      device: '',
      os_version: '',
      app_version: appVersion,
    })
    setScreenshot(null)
    setScreenshotError(null)
    mutation.reset()
  }

  function chooseScreenshot(file: File | null) {
    const problem = file ? screenshotProblem(file) : null
    setScreenshotError(problem)
    setScreenshot(problem ? null : file)
  }

  async function onSubmit(values: FeedbackValues) {
    const screenshotBase64 = screenshot
      ? await readFileAsBase64(screenshot)
      : null
    mutation.mutate(
      {
        message: values.message,
        device: optional(values.device),
        os_version: optional(values.os_version),
        app_version: optional(values.app_version),
        artifact_id: artifactId ?? null,
        screenshot_base64: screenshotBase64,
      },
      {
        onSuccess: () => {
          toast.success('Feedback sent', {
            description: 'The team can see it on this build now.',
          })
          setOpen(false)
          reset()
        },
      },
    )
  }

  const sending = mutation.isPending || form.formState.isSubmitting

  return (
    <Drawer
      open={open}
      onOpenChange={(nextOpen) => {
        if (nextOpen) prefillDevice()
        setOpen(nextOpen)
      }}
      showSwipeHandle={isMobile}
      swipeDirection={isMobile ? 'down' : 'right'}
    >
      <DrawerTrigger render={children} />
      <DrawerContent>
        <DrawerHeader>
          <DrawerTitle>Send feedback</DrawerTitle>
          <DrawerDescription>
            Your note goes to the team with this build.
          </DrawerDescription>
        </DrawerHeader>
        <Form {...form}>
          <form
            onSubmit={form.handleSubmit(onSubmit)}
            className="flex min-h-0 flex-1 flex-col"
          >
            <div className="flex-1 scroll-fade space-y-4 overflow-y-auto p-4">
              <FormField
                control={form.control}
                name="message"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel>What happened?</FormLabel>
                    <FormControl>
                      <Textarea
                        {...field}
                        rows={5}
                        placeholder="Steps, what you expected, and what you saw"
                        className="min-h-28"
                      />
                    </FormControl>
                    <FormMessage />
                  </FormItem>
                )}
              />
              <div className="grid gap-4 sm:grid-cols-2">
                <FormField
                  control={form.control}
                  name="device"
                  render={({ field }) => (
                    <FormItem>
                      <FormLabel>Device</FormLabel>
                      <FormControl>
                        <Input {...field} placeholder="Pixel 8" />
                      </FormControl>
                      <FormMessage />
                    </FormItem>
                  )}
                />
                <FormField
                  control={form.control}
                  name="os_version"
                  render={({ field }) => (
                    <FormItem>
                      <FormLabel>OS version</FormLabel>
                      <FormControl>
                        <Input {...field} placeholder="Android 15" />
                      </FormControl>
                      <FormMessage />
                    </FormItem>
                  )}
                />
              </div>
              <FormField
                control={form.control}
                name="app_version"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel>App version</FormLabel>
                    <FormControl>
                      <Input {...field} className="font-mono" />
                    </FormControl>
                    <FormDescription>
                      Filled in from this release. Change it if you tested
                      another version.
                    </FormDescription>
                    <FormMessage />
                  </FormItem>
                )}
              />
              <ScreenshotField
                file={screenshot}
                error={screenshotError}
                onChange={chooseScreenshot}
              />
              {mutation.error ? (
                <Alert variant="destructive">
                  <AlertDescription>
                    {getApiErrorMessage(mutation.error, FEEDBACK_ERRORS)}
                  </AlertDescription>
                </Alert>
              ) : null}
            </div>
            <DrawerFooter>
              <DrawerClose
                render={
                  <Button type="button" variant="outline">
                    Cancel
                  </Button>
                }
              />
              <Button type="submit" disabled={sending}>
                {sending ? (
                  <>
                    <Spinner className="size-4" />
                    Sending...
                  </>
                ) : (
                  'Send feedback'
                )}
              </Button>
            </DrawerFooter>
          </form>
        </Form>
      </DrawerContent>
    </Drawer>
  )
}
