import { useState } from 'react'
import { useForm } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import { z } from 'zod'
import { toast } from 'sonner'

import { Alert, AlertDescription } from '@/components/ui/alert'
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
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
} from '@/components/ui/form'
import { Input } from '@/components/ui/input'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Separator } from '@/components/ui/separator'
import { Spinner } from '@/components/ui/spinner'
import { Textarea } from '@/components/ui/textarea'
import { useCreatePipeline, useValidatePipeline } from '@/hooks/use-pipelines'
import type {
  BuildPlatform,
  ConcurrencyPolicy,
  CreatePipelineRequest,
  TriggerConfig,
} from '@/lib/types'

const createPipelineSchema = z
  .object({
    name: z.string().min(1, 'Name is required'),
    config_mode: z.enum(['auto', 'explicit']),
    config_path: z.string().optional(),
    platform_android: z.boolean(),
    platform_ios: z.boolean(),
    platform_macos: z.boolean(),
    flutter_version: z
      .string()
      .optional()
      .refine((v) => !v || v.trim().length <= 64, 'Max 64 characters'),
    enable_customization: z.boolean(),
    pre_build_commands: z.string().optional(),
    build_commands: z.string().optional(),
    post_build_commands: z.string().optional(),
    android_build_args: z.string().optional(),
    ios_build_args: z.string().optional(),
    macos_build_args: z.string().optional(),
    android_command_override: z.string().optional(),
    ios_command_override: z.string().optional(),
    macos_command_override: z.string().optional(),
    env_vars: z.string().optional(),
    artifact_patterns: z.string().optional(),
    branches: z.string().optional(),
    max_concurrent: z
      .string()
      .optional()
      .refine(
        (v) => !v || (/^\d+$/.test(v) && Number(v) >= 1 && Number(v) <= 100),
        'Must be a number between 1 and 100',
      ),
  })
  .superRefine((data, ctx) => {
    if (data.config_mode === 'explicit' && !data.config_path?.trim()) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        message: 'Config path is required when explicit mode is selected',
        path: ['config_path'],
      })
    }
  })

type CreatePipelineForm = z.infer<typeof createPipelineSchema>

const TRIGGER_EVENTS = ['push', 'pull_request', 'tag_push'] as const
const STEP_LABELS = ['Basics', 'Build Setup', 'Triggers'] as const

interface CreatePipelineDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  projectId: string
}

function parseMultiline(raw?: string): string[] {
  if (!raw) return []
  return raw
    .split('\n')
    .map((line) => line.trim())
    .filter(Boolean)
}

function parseCsv(raw?: string): string[] {
  if (!raw) return []
  return raw
    .split(',')
    .map((value) => value.trim())
    .filter(Boolean)
}

function parseEnvVars(raw?: string): Array<{ key: string; value: string }> {
  if (!raw) return []
  return raw
    .split('\n')
    .map((line) => line.trim())
    .filter(Boolean)
    .map((line) => {
      const eq = line.indexOf('=')
      if (eq <= 0) {
        return { key: line.trim(), value: '' }
      }
      return {
        key: line.slice(0, eq).trim(),
        value: line.slice(eq + 1).trim(),
      }
    })
}

function selectedPlatforms(data: CreatePipelineForm): BuildPlatform[] {
  const platforms: BuildPlatform[] = []
  if (data.platform_android) platforms.push('android')
  if (data.platform_ios) platforms.push('ios')
  if (data.platform_macos) platforms.push('macos')
  return platforms
}

function applyArgs(base: string, args: string[]): string {
  if (args.length === 0) return base
  return `${base} ${args.join(' ')}`
}

function previewPlatformCommands(data: CreatePipelineForm): string[] {
  const platforms = selectedPlatforms(data)
  const commands: string[] = []
  for (const platform of platforms) {
    if (platform === 'android') {
      commands.push(
        data.android_command_override?.trim() ||
          applyArgs('flutter build apk --release', parseMultiline(data.android_build_args)),
      )
    }
    if (platform === 'ios') {
      commands.push(
        data.ios_command_override?.trim() ||
          applyArgs(
            'flutter build ios --release --no-codesign',
            parseMultiline(data.ios_build_args),
          ),
      )
    }
    if (platform === 'macos') {
      commands.push(
        data.macos_command_override?.trim() ||
          applyArgs('flutter build macos --release', parseMultiline(data.macos_build_args)),
      )
    }
  }
  return commands.filter(Boolean)
}

function defaultArtifactPatterns(platforms: BuildPlatform[]): string[] {
  const patterns = new Set<string>()
  if (platforms.includes('android')) patterns.add('*.apk')
  if (platforms.includes('ios')) patterns.add('*.ipa')
  if (platforms.includes('macos')) patterns.add('*.app')
  if (patterns.size === 0) patterns.add('*.apk')
  return [...patterns]
}

export default function CreatePipelineDialog({
  open,
  onOpenChange,
  projectId,
}: CreatePipelineDialogProps) {
  const createMutation = useCreatePipeline()
  const validateMutation = useValidatePipeline()

  const form = useForm<CreatePipelineForm>({
    resolver: zodResolver(createPipelineSchema),
    defaultValues: {
      name: '',
      config_mode: 'auto',
      config_path: '.oore.yaml',
      platform_android: true,
      platform_ios: false,
      platform_macos: false,
      flutter_version: '',
      enable_customization: false,
      pre_build_commands: '',
      build_commands: '',
      post_build_commands: '',
      android_build_args: '',
      ios_build_args: '',
      macos_build_args: '',
      android_command_override: '',
      ios_command_override: '',
      macos_command_override: '',
      env_vars: '',
      artifact_patterns: '*.apk',
      branches: '',
      max_concurrent: undefined,
    },
    mode: 'onBlur',
    shouldUnregister: false,
  })

  const [selectedEvents, setSelectedEvents] = useState<string[]>(['push'])
  const [cancelPrevious, setCancelPrevious] = useState(true)
  const [validationErrors, setValidationErrors] = useState<string[]>([])
  const [step, setStep] = useState(0)

  function buildPayload(data: CreatePipelineForm): CreatePipelineRequest {
    const trigger_config: TriggerConfig = {
      events: selectedEvents,
      branches: parseCsv(data.branches),
    }

    const concurrency: ConcurrencyPolicy = {
      cancel_previous: cancelPrevious,
      max_concurrent: data.max_concurrent ? Number(data.max_concurrent) : undefined,
    }

    const platforms = selectedPlatforms(data)
    const commands = data.enable_customization
      ? {
          pre_build: parseMultiline(data.pre_build_commands),
          build: parseMultiline(data.build_commands),
          post_build: parseMultiline(data.post_build_commands),
        }
      : {
          pre_build: [],
          build: [],
          post_build: [],
        }

    const customPatterns = data.enable_customization
      ? parseMultiline(data.artifact_patterns)
      : []

    return {
      name: data.name.trim(),
      config_path:
        data.config_mode === 'explicit' ? data.config_path?.trim() : '.oore.yaml',
      config_path_explicit: data.config_mode === 'explicit',
      execution_config: {
        platforms,
        flutter_version: data.flutter_version?.trim() || undefined,
        commands,
        platform_build_args: data.enable_customization
          ? {
              android: parseMultiline(data.android_build_args),
              ios: parseMultiline(data.ios_build_args),
              macos: parseMultiline(data.macos_build_args),
            }
          : { android: [], ios: [], macos: [] },
        platform_commands: data.enable_customization
          ? {
              android: data.android_command_override?.trim() || undefined,
              ios: data.ios_command_override?.trim() || undefined,
              macos: data.macos_command_override?.trim() || undefined,
            }
          : {},
        env: data.enable_customization ? parseEnvVars(data.env_vars) : [],
        artifact_patterns:
          customPatterns.length > 0
            ? customPatterns
            : defaultArtifactPatterns(platforms),
      },
      trigger_config,
      concurrency,
    }
  }

  async function onSubmit(data: CreatePipelineForm) {
    const payload = buildPayload(data)

    if (payload.execution_config?.platforms.length === 0) {
      setValidationErrors(['Pick at least one platform to build'])
      return
    }

    try {
      const result = await validateMutation.mutateAsync(payload)
      if (!result.valid && result.errors?.length) {
        setValidationErrors(result.errors)
        return
      }
    } catch {
      // Validation endpoint is best-effort.
    }

    setValidationErrors([])

    createMutation.mutate(
      { projectId, data: payload },
      {
        onSuccess: () => {
          toast.success('Pipeline created')
          handleClose()
        },
        onError: (err) => {
          toast.error(`Failed to create pipeline: ${err.message}`)
        },
      },
    )
  }

  function handleClose() {
    form.reset()
    setSelectedEvents(['push'])
    setCancelPrevious(true)
    setValidationErrors([])
    setStep(0)
    onOpenChange(false)
  }

  async function goToNextStep() {
    const values = form.getValues()

    if (step === 0) {
      const valid = await form.trigger([
        'name',
        'config_mode',
        'config_path',
        'platform_android',
        'platform_ios',
        'platform_macos',
        'flutter_version',
      ])
      if (!valid) return
      if (selectedPlatforms(values).length === 0) {
        setValidationErrors(['Pick at least one platform to build'])
        return
      }
    }

    if (step === 1) {
      const valid = await form.trigger([
        'enable_customization',
        'pre_build_commands',
        'build_commands',
        'post_build_commands',
        'android_build_args',
        'ios_build_args',
        'macos_build_args',
        'android_command_override',
        'ios_command_override',
        'macos_command_override',
        'env_vars',
        'artifact_patterns',
      ])
      if (!valid) return
    }

    setValidationErrors([])
    setStep((current) => Math.min(current + 1, STEP_LABELS.length - 1))
  }

  function toggleEvent(event: string) {
    setSelectedEvents((prev) =>
      prev.includes(event) ? prev.filter((entry) => entry !== event) : [...prev, event],
    )
  }

  const values = form.watch()
  const configMode = values.config_mode
  const customEnabled = values.enable_customization
  const previewDefaults = previewPlatformCommands(values)

  return (
    <Dialog
      open={open}
      onOpenChange={(nextOpen) => {
        if (!nextOpen) handleClose()
        else onOpenChange(true)
      }}
    >
      <DialogContent className="max-h-[90vh] overflow-y-auto sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>Create Pipeline</DialogTitle>
          <DialogDescription>
            Set up your workflow in 3 quick steps.
          </DialogDescription>
        </DialogHeader>

        <div className="grid grid-cols-3 gap-2">
          {STEP_LABELS.map((label, index) => (
            <Button
              key={label}
              type="button"
              size="sm"
              variant={index === step ? 'default' : 'outline'}
              className="justify-start"
              onClick={() => setStep(index)}
            >
              {index + 1}. {label}
            </Button>
          ))}
        </div>

        <Form {...form}>
          <form onSubmit={form.handleSubmit(onSubmit)} className="space-y-4">
            {step === 0 ? (
              <section className="space-y-4">
                <FormField
                  control={form.control}
                  name="name"
                  render={({ field }) => (
                    <FormItem>
                      <FormLabel>Workflow name</FormLabel>
                      <FormControl>
                        <Input placeholder="Release" autoFocus {...field} />
                      </FormControl>
                      <FormMessage />
                    </FormItem>
                  )}
                />

                <FormField
                  control={form.control}
                  name="config_mode"
                  render={({ field }) => (
                    <FormItem>
                      <FormLabel>Config source</FormLabel>
                      <Select value={field.value} onValueChange={field.onChange}>
                        <FormControl>
                          <SelectTrigger className="w-full">
                            <SelectValue placeholder="Choose source" />
                          </SelectTrigger>
                        </FormControl>
                        <SelectContent>
                          <SelectItem value="auto">
                            Use repo config if found (.oore.yaml, .oore.yml)
                          </SelectItem>
                          <SelectItem value="explicit">
                            Use a specific config file path
                          </SelectItem>
                        </SelectContent>
                      </Select>
                      <FormMessage />
                    </FormItem>
                  )}
                />

                {configMode === 'explicit' ? (
                  <FormField
                    control={form.control}
                    name="config_path"
                    render={({ field }) => (
                      <FormItem>
                        <FormLabel>Config path</FormLabel>
                        <FormControl>
                          <Input placeholder="ci/oore.yaml" {...field} />
                        </FormControl>
                        <FormMessage />
                      </FormItem>
                    )}
                  />
                ) : null}

                <div className="space-y-2">
                  <FormLabel>Build for platforms</FormLabel>
                  <div className="grid grid-cols-1 gap-2 sm:grid-cols-3">
                    <FormField
                      control={form.control}
                      name="platform_android"
                      render={({ field }) => (
                        <label className="flex items-center gap-2 text-sm">
                          <Checkbox
                            checked={field.value}
                            onCheckedChange={(checked) => field.onChange(!!checked)}
                          />
                          Android
                        </label>
                      )}
                    />
                    <FormField
                      control={form.control}
                      name="platform_ios"
                      render={({ field }) => (
                        <label className="flex items-center gap-2 text-sm">
                          <Checkbox
                            checked={field.value}
                            onCheckedChange={(checked) => field.onChange(!!checked)}
                          />
                          iOS
                        </label>
                      )}
                    />
                    <FormField
                      control={form.control}
                      name="platform_macos"
                      render={({ field }) => (
                        <label className="flex items-center gap-2 text-sm">
                          <Checkbox
                            checked={field.value}
                            onCheckedChange={(checked) => field.onChange(!!checked)}
                          />
                          macOS
                        </label>
                      )}
                    />
                  </div>
                </div>

                <FormField
                  control={form.control}
                  name="flutter_version"
                  render={({ field }) => (
                    <FormItem>
                      <FormLabel>Flutter version (optional)</FormLabel>
                      <FormControl>
                        <Input
                          placeholder="3.24.0 (or stable)"
                          className="font-mono"
                          {...field}
                        />
                      </FormControl>
                      <p className="text-xs text-muted-foreground">
                        If set, runner executes Flutter commands using{' '}
                        <span className="font-mono">fvm use &lt;version&gt;</span>.
                        If unset, oore will auto-read <span className="font-mono">.fvmrc</span>{' '}
                        from the repo when present.
                      </p>
                      <FormMessage />
                    </FormItem>
                  )}
                />
              </section>
            ) : null}

            {step === 1 ? (
              <section className="space-y-4">
                <div className="space-y-2 rounded-md border p-3">
                  <p className="text-sm font-medium">Default commands</p>
                  <p className="text-xs text-muted-foreground">
                    We always run <span className="font-mono">flutter pub get</span> first.
                  </p>
                  {previewDefaults.length > 0 ? (
                    <ul className="list-disc space-y-1 pl-4 text-xs text-muted-foreground">
                      {previewDefaults.map((command) => (
                        <li key={command} className="font-mono">
                          {command}
                        </li>
                      ))}
                    </ul>
                  ) : (
                    <p className="text-xs text-muted-foreground">
                      Pick a platform in step 1.
                    </p>
                  )}
                </div>

                <Separator />

                <FormField
                  control={form.control}
                  name="enable_customization"
                  render={({ field }) => (
                    <FormItem>
                      <label className="flex items-center gap-2 text-sm font-medium">
                        <Checkbox
                          checked={field.value}
                          onCheckedChange={(checked) => field.onChange(!!checked)}
                        />
                        Customize build script (optional)
                      </label>
                    </FormItem>
                  )}
                />

                {customEnabled ? (
                  <>
                    <FormField
                      control={form.control}
                      name="pre_build_commands"
                      render={({ field }) => (
                        <FormItem>
                          <FormLabel>Pre-build commands</FormLabel>
                          <FormControl>
                            <Textarea
                              placeholder={'One command per line\nexample: dart --version'}
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
                      name="build_commands"
                      render={({ field }) => (
                        <FormItem>
                          <FormLabel>Extra build commands</FormLabel>
                          <FormControl>
                            <Textarea
                              placeholder={'One command per line\nexample: flutter test'}
                              className="font-mono"
                              {...field}
                            />
                          </FormControl>
                          <p className="text-xs text-muted-foreground">
                            Runs after default platform commands.
                          </p>
                          <FormMessage />
                        </FormItem>
                      )}
                    />

                    <FormField
                      control={form.control}
                      name="post_build_commands"
                      render={({ field }) => (
                        <FormItem>
                          <FormLabel>Post-build commands</FormLabel>
                          <FormControl>
                            <Textarea
                              placeholder={'One command per line\nexample: ls -la build/'}
                              className="font-mono"
                              {...field}
                            />
                          </FormControl>
                          <FormMessage />
                        </FormItem>
                      )}
                    />

                    <Separator />
                    <p className="text-sm font-medium">Per-platform build arguments</p>
                    <p className="text-xs text-muted-foreground">
                      Each line is appended to the platform's default Flutter build command.
                    </p>

                    <FormField
                      control={form.control}
                      name="android_build_args"
                      render={({ field }) => (
                        <FormItem>
                          <FormLabel>Android build args</FormLabel>
                          <FormControl>
                            <Textarea
                              placeholder={
                                '--dart-define-from-file=config/dev.json\n--build-name=1.0.$PROJECT_BUILD_NUMBER\n--build-number=$PROJECT_BUILD_NUMBER\n--obfuscate\n--split-debug-info=build/app/outputs/android/symbols'
                              }
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
                      name="ios_build_args"
                      render={({ field }) => (
                        <FormItem>
                          <FormLabel>iOS build args</FormLabel>
                          <FormControl>
                            <Textarea
                              placeholder={'--flavor=production\n--build-number=$PROJECT_BUILD_NUMBER'}
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
                      name="macos_build_args"
                      render={({ field }) => (
                        <FormItem>
                          <FormLabel>macOS build args</FormLabel>
                          <FormControl>
                            <Textarea
                              placeholder={'--build-number=$PROJECT_BUILD_NUMBER'}
                              className="font-mono"
                              {...field}
                            />
                          </FormControl>
                          <FormMessage />
                        </FormItem>
                      )}
                    />

                    <Separator />
                    <p className="text-sm font-medium">Per-platform command override (optional)</p>
                    <p className="text-xs text-muted-foreground">
                      If set, this command fully replaces the default platform command.
                    </p>

                    <FormField
                      control={form.control}
                      name="android_command_override"
                      render={({ field }) => (
                        <FormItem>
                          <FormLabel>Android command override</FormLabel>
                          <FormControl>
                            <Input
                              placeholder="flutter build appbundle --release"
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
                      name="ios_command_override"
                      render={({ field }) => (
                        <FormItem>
                          <FormLabel>iOS command override</FormLabel>
                          <FormControl>
                            <Input
                              placeholder="flutter build ios --release --no-codesign"
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
                      name="macos_command_override"
                      render={({ field }) => (
                        <FormItem>
                          <FormLabel>macOS command override</FormLabel>
                          <FormControl>
                            <Input
                              placeholder="flutter build macos --release"
                              className="font-mono"
                              {...field}
                            />
                          </FormControl>
                          <FormMessage />
                        </FormItem>
                      )}
                    />

                    <Separator />

                    <FormField
                      control={form.control}
                      name="env_vars"
                      render={({ field }) => (
                        <FormItem>
                          <FormLabel>Environment variables</FormLabel>
                          <FormControl>
                            <Textarea
                              placeholder={'API_BASE_URL=https://api.example.com\nAPP_FLAVOR=dev'}
                              className="font-mono"
                              {...field}
                            />
                          </FormControl>
                          <p className="text-xs text-muted-foreground">
                            One `KEY=VALUE` per line. Available in all build steps.
                          </p>
                          <FormMessage />
                        </FormItem>
                      )}
                    />

                    <FormField
                      control={form.control}
                      name="artifact_patterns"
                      render={({ field }) => (
                        <FormItem>
                          <FormLabel>Artifact patterns</FormLabel>
                          <FormControl>
                            <Textarea
                              placeholder={'One pattern per line\n*.apk\n*.ipa'}
                              className="font-mono"
                              {...field}
                            />
                          </FormControl>
                          <FormMessage />
                        </FormItem>
                      )}
                    />
                  </>
                ) : (
                  <p className="text-xs text-muted-foreground">
                    Using sensible defaults. You can add custom commands anytime.
                  </p>
                )}
              </section>
            ) : null}

            {step === 2 ? (
              <section className="space-y-4">
                <div className="space-y-2">
                  <FormLabel>Trigger events</FormLabel>
                  <div className="grid grid-cols-1 gap-2 sm:grid-cols-3">
                    {TRIGGER_EVENTS.map((event) => (
                      <label key={event} className="flex items-center gap-2 text-sm">
                        <Checkbox
                          checked={selectedEvents.includes(event)}
                          onCheckedChange={() => toggleEvent(event)}
                        />
                        {event}
                      </label>
                    ))}
                  </div>
                </div>

                <FormField
                  control={form.control}
                  name="branches"
                  render={({ field }) => (
                    <FormItem>
                      <FormLabel>Branch patterns (optional)</FormLabel>
                      <FormControl>
                        <Input placeholder="main, develop, release/*" {...field} />
                      </FormControl>
                      <FormMessage />
                    </FormItem>
                  )}
                />

                <div className="space-y-2">
                  <label className="flex items-center gap-2 text-sm">
                    <Checkbox
                      checked={cancelPrevious}
                      onCheckedChange={(checked) => setCancelPrevious(!!checked)}
                    />
                    Cancel previous builds on same branch
                  </label>
                </div>

                <FormField
                  control={form.control}
                  name="max_concurrent"
                  render={({ field }) => (
                    <FormItem>
                      <FormLabel>Max concurrent builds (optional)</FormLabel>
                      <FormControl>
                        <Input type="number" min={1} max={100} {...field} />
                      </FormControl>
                      <FormMessage />
                    </FormItem>
                  )}
                />
              </section>
            ) : null}

            {validationErrors.length > 0 ? (
              <Alert variant="destructive">
                <AlertDescription>
                  <ul className="list-disc space-y-1 pl-4">
                    {validationErrors.map((err, index) => (
                      <li key={index}>{err}</li>
                    ))}
                  </ul>
                </AlertDescription>
              </Alert>
            ) : null}

            <DialogFooter>
              <Button type="button" variant="outline" onClick={handleClose}>
                Cancel
              </Button>
              {step > 0 ? (
                <Button
                  type="button"
                  variant="outline"
                  onClick={() => setStep((current) => Math.max(current - 1, 0))}
                >
                  Back
                </Button>
              ) : null}
              {step < STEP_LABELS.length - 1 ? (
                <Button type="button" onClick={goToNextStep}>
                  Next
                </Button>
              ) : (
                <Button
                  type="button"
                  disabled={createMutation.isPending || validateMutation.isPending}
                  onClick={() => {
                    void form.handleSubmit(onSubmit)()
                  }}
                >
                  {createMutation.isPending ? (
                    <>
                      <Spinner className="size-4" />
                      Saving...
                    </>
                  ) : (
                    'Save'
                  )}
                </Button>
              )}
            </DialogFooter>
          </form>
        </Form>
      </DialogContent>
    </Dialog>
  )
}
