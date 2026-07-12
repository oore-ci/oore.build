import { useState } from 'react'
import { useForm } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  AlertCircleIcon,
  ArrowDown01Icon,
  ArrowUp01Icon,
} from '@hugeicons/core-free-icons'

import type { PipelineFormValues } from '@/lib/pipeline-schema'
import { useMountEffect } from '@/hooks/use-mount-effect'
import { useWindowEvent } from '@/hooks/use-window-event'
import { Button } from '@/components/ui/button'
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/components/ui/card'
import { Checkbox } from '@/components/ui/checkbox'
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible'
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
import { Badge } from '@/components/ui/badge'
import { Alert, AlertDescription } from '@/components/ui/alert'
import SetupHint from '@/components/setup-hint'
import { TRIGGER_EVENTS, pipelineFormSchema } from '@/lib/pipeline-schema'
import {
  parseEnvVars,
  parseMultiline,
  previewPlatformCommands,
  selectedPlatforms,
} from '@/lib/pipeline-form-utils'

interface PipelineFormProps {
  initialValues: PipelineFormValues
  initialEvents: Array<string>
  initialCancelPrevious: boolean
  onSubmit: (
    data: PipelineFormValues,
    events: Array<string>,
    cancelPrevious: boolean,
    releaseKeystoreFile: File | null,
    debugKeystoreFile: File | null,
    iosSigningFiles: {
      p12File: File | null
      apiKeyFile: File | null
      profileFiles: Record<string, File | null>
    },
  ) => Promise<void>
  onCancel: () => void
  submitLabel: string
  isPending: boolean
  validationErrors?: Array<string>
  /** Content rendered after all form sections but before the sticky action bar */
  children?: React.ReactNode
  /** Local-mode repositories only support manual/API build triggers for now. */
  manualOnlyTriggers?: boolean
  readOnly?: boolean
  readOnlyReason?: string
  retrySigning?: 'android' | 'ios'
  signingError?: string
  signingData?: {
    release: {
      has_keystore: boolean
      keystore_filename?: string
      has_store_password: boolean
      has_key_password: boolean
    }
    debug: {
      has_keystore: boolean
      keystore_filename?: string
      has_store_password: boolean
      has_key_password: boolean
    }
  }
  iosSigningData?: {
    enabled: boolean
    mode: 'manual' | 'api' | 'hybrid'
    team_id?: string
    bundle_ids: Array<string>
    has_p12: boolean
    p12_filename?: string
    has_p12_password: boolean
    has_api_key: boolean
    api_key_id?: string
    api_issuer_id?: string
    provisioning_profiles: Array<{
      bundle_id: string
      has_profile: boolean
      profile_filename?: string
      profile_uuid?: string
      profile_name?: string
      expires_at?: number
    }>
  }
}

function parseBundleIdsInput(raw?: string): Array<string> {
  if (!raw) return []
  const seen = new Set<string>()
  const values: Array<string> = []
  for (const part of raw.split(/[\n,]/g)) {
    const trimmed = part.trim()
    if (!trimmed) continue
    if (seen.has(trimmed)) continue
    seen.add(trimmed)
    values.push(trimmed)
  }
  return values
}

const SIGNING_MODES: Record<string, string> = {
  manual: 'Manual (.p12 + provisioning profiles)',
  api: 'API (App Store Connect automation)',
  hybrid: 'Hybrid (manual cert + API automation)',
}

const CONFIG_SOURCES: Record<string, string> = {
  auto: 'Use repo config if found (.oore.yaml, .oore.yml)',
  explicit: 'Use a specific config file path',
}

const ANDROID_GRADLE_SIGNING_SNIPPET = `android {
    signingConfigs {
        release {
            storeFile file(System.getenv("OORE_ANDROID_KEYSTORE_PATH"))
            storePassword System.getenv("OORE_ANDROID_KEYSTORE_PASSWORD")
            keyAlias System.getenv("OORE_ANDROID_KEY_ALIAS")
            keyPassword System.getenv("OORE_ANDROID_KEY_PASSWORD")
        }
    }
    buildTypes {
        release {
            signingConfig signingConfigs.release
        }
    }
}`

function SectionHeader({
  title,
  summary,
  open,
  errorCount,
}: {
  title: string
  summary?: string
  open: boolean
  errorCount?: number
}) {
  return (
    <div className="flex w-full items-center justify-between">
      <div className="flex items-center gap-2">
        <CardTitle className="text-sm font-medium uppercase tracking-wider text-muted-foreground">
          {title}
        </CardTitle>
        {errorCount && errorCount > 0 ? (
          <Badge variant="destructive" className="text-[10px]">
            {errorCount} {errorCount === 1 ? 'error' : 'errors'}
          </Badge>
        ) : null}
      </div>
      <div className="flex items-center gap-2">
        {!open && summary ? (
          <CardDescription className="text-xs">{summary}</CardDescription>
        ) : null}
        <HugeiconsIcon
          icon={open ? ArrowUp01Icon : ArrowDown01Icon}
          size={16}
          className="text-muted-foreground"
        />
      </div>
    </div>
  )
}

export default function PipelineForm({
  initialValues,
  initialEvents,
  initialCancelPrevious,
  onSubmit,
  onCancel,
  submitLabel,
  isPending,
  validationErrors = [],
  children,
  manualOnlyTriggers = false,
  readOnly = false,
  readOnlyReason,
  retrySigning,
  signingError,
  signingData,
  iosSigningData,
}: PipelineFormProps) {
  const form = useForm<PipelineFormValues>({
    resolver: zodResolver(pipelineFormSchema),
    defaultValues: initialValues,
    mode: 'onBlur',
    shouldUnregister: false,
  })

  const [selectedEvents, setSelectedEvents] =
    useState<Array<string>>(initialEvents)
  const [cancelPrevious, setCancelPrevious] = useState(initialCancelPrevious)
  const [releaseKeystoreFile, setReleaseKeystoreFile] = useState<File | null>(
    null,
  )
  const [debugKeystoreFile, setDebugKeystoreFile] = useState<File | null>(null)
  const [iosP12File, setIosP12File] = useState<File | null>(null)
  const [iosApiKeyFile, setIosApiKeyFile] = useState<File | null>(null)
  const [iosProfileFiles, setIosProfileFiles] = useState<
    Record<string, File | null>
  >({})
  const [isDirty, setIsDirty] = useState(false)

  const [configOpen, setConfigOpen] = useState(true)
  const [triggersOpen, setTriggersOpen] = useState(true)
  const [commandsOpen, setCommandsOpen] = useState(false)
  const [platformArgsOpen, setPlatformArgsOpen] = useState(false)
  const [envOpen, setEnvOpen] = useState(false)
  const [artifactsOpen, setArtifactsOpen] = useState(false)
  const [iosSigningOpen, setIosSigningOpen] = useState(
    () => !!initialValues.ios_signing_enabled || retrySigning === 'ios',
  )
  const [signingOpen, setSigningOpen] = useState(
    () =>
      retrySigning === 'android' ||
      !!(
        initialValues.android_signing_release_enabled ||
        initialValues.android_signing_debug_enabled
      ),
  )

  useMountEffect(() => {
    const subscription = form.watch(() => setIsDirty(true))
    return () => subscription.unsubscribe()
  })

  useWindowEvent('beforeunload', (event) => {
    if (isDirty) event.preventDefault()
  })

  function toggleEvent(event: string) {
    setSelectedEvents((prev) =>
      prev.includes(event)
        ? prev.filter((entry) => entry !== event)
        : [...prev, event],
    )
    setIsDirty(true)
  }

  async function handleFormSubmit(data: PipelineFormValues) {
    await onSubmit(
      data,
      selectedEvents,
      cancelPrevious,
      releaseKeystoreFile,
      debugKeystoreFile,
      {
        p12File: iosP12File,
        apiKeyFile: iosApiKeyFile,
        profileFiles: iosProfileFiles,
      },
    )
  }

  async function handleValidate() {
    const valid = await form.trigger()
    if (!valid) {
      const errors = form.formState.errors
      if (
        errors.name ||
        errors.config_mode ||
        errors.config_path ||
        errors.flutter_version
      ) {
        setConfigOpen(true)
      }
      if (errors.max_concurrent || errors.branches) {
        setTriggersOpen(true)
      }
      if (
        errors.pre_build_commands ||
        errors.build_commands ||
        errors.post_build_commands
      ) {
        setCommandsOpen(true)
      }
      if (
        errors.ios_signing_enabled ||
        errors.ios_signing_team_id ||
        errors.ios_signing_bundle_ids
      ) {
        setIosSigningOpen(true)
      }
    }
  }

  const values = form.watch()
  const configMode = values.config_mode
  const previewDefaults = previewPlatformCommands(values)

  const envVarCount = parseEnvVars(values.env_vars).length
  const artifactPatterns = parseMultiline(values.artifact_patterns)
  const platforms = selectedPlatforms(values)
  const preBuildCount = parseMultiline(values.pre_build_commands).length
  const buildCmdCount = parseMultiline(values.build_commands).length
  const postBuildCount = parseMultiline(values.post_build_commands).length
  const totalCmdCount = preBuildCount + buildCmdCount + postBuildCount
  const iosBundleIds = parseBundleIdsInput(values.ios_signing_bundle_ids)
  const iosSigningMode = values.ios_signing_mode
  const iosProfilesByBundle = new Map(
    (iosSigningData?.provisioning_profiles ?? []).map((profile) => [
      profile.bundle_id,
      profile,
    ]),
  )

  return (
    <Form {...form}>
      <form
        onSubmit={form.handleSubmit(handleFormSubmit)}
        className="space-y-4 pb-24"
      >
        {/* Name - always visible */}
        <Card>
          <CardContent>
            <FormField
              control={form.control}
              name="name"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>Pipeline name</FormLabel>
                  <FormControl>
                    <Input placeholder="Release" autoFocus {...field} />
                  </FormControl>
                  <FormMessage />
                </FormItem>
              )}
            />
          </CardContent>
        </Card>

        {/* Configuration section */}
        <Collapsible open={configOpen} onOpenChange={setConfigOpen}>
          <Card>
            <CollapsibleTrigger className="w-full cursor-pointer">
              <CardHeader>
                <SectionHeader
                  title="Configuration"
                  summary={`${platforms.length} platform${platforms.length !== 1 ? 's' : ''}, ${configMode === 'explicit' ? 'explicit config' : 'auto-detect'}`}
                  open={configOpen}
                />
              </CardHeader>
            </CollapsibleTrigger>
            <CollapsibleContent>
              <CardContent className="space-y-4">
                <FormField
                  control={form.control}
                  name="config_mode"
                  render={({ field }) => (
                    <FormItem>
                      <FormLabel>Config source</FormLabel>
                      <Select
                        value={field.value}
                        onValueChange={field.onChange}
                        items={CONFIG_SOURCES}
                      >
                        <FormControl>
                          <SelectTrigger className="w-full">
                            <SelectValue placeholder="Choose source" />
                          </SelectTrigger>
                        </FormControl>
                        <SelectContent>
                          {Object.entries(CONFIG_SOURCES).map(
                            ([key, value]) => (
                              <SelectItem key={key} value={key}>
                                {value}
                              </SelectItem>
                            ),
                          )}
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
                            onCheckedChange={(checked) =>
                              field.onChange(!!checked)
                            }
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
                            onCheckedChange={(checked) =>
                              field.onChange(!!checked)
                            }
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
                            onCheckedChange={(checked) =>
                              field.onChange(!!checked)
                            }
                          />
                          macOS
                        </label>
                      )}
                    />
                  </div>
                  <p className="text-xs text-muted-foreground">
                    Select the platforms you want to build for. You can change
                    this later.
                  </p>
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
                        <span className="font-mono">
                          fvm use &lt;version&gt;
                        </span>
                        . If unset, oore will auto-read{' '}
                        <span className="font-mono">.fvmrc</span> from the repo
                        when present.
                      </p>
                      <FormMessage />
                    </FormItem>
                  )}
                />

                {previewDefaults.length > 0 ? (
                  <div className="space-y-1 border p-3">
                    <p className="text-xs font-medium text-muted-foreground">
                      Default build commands
                    </p>
                    <ul className="list-disc space-y-0.5 pl-4 text-xs text-muted-foreground">
                      {previewDefaults.map((command) => (
                        <li key={command} className="font-mono">
                          {command}
                        </li>
                      ))}
                    </ul>
                  </div>
                ) : null}
              </CardContent>
            </CollapsibleContent>
          </Card>
        </Collapsible>

        {/* Triggers section */}
        <Collapsible open={triggersOpen} onOpenChange={setTriggersOpen}>
          <Card>
            <CollapsibleTrigger className="w-full cursor-pointer">
              <CardHeader>
                <SectionHeader
                  title="Triggers"
                  summary={
                    manualOnlyTriggers
                      ? `manual only, cancel previous: ${cancelPrevious ? 'on' : 'off'}`
                      : `${selectedEvents.length} event${selectedEvents.length !== 1 ? 's' : ''}, cancel previous: ${cancelPrevious ? 'on' : 'off'}`
                  }
                  open={triggersOpen}
                />
              </CardHeader>
            </CollapsibleTrigger>
            <CollapsibleContent>
              <CardContent className="space-y-4">
                {manualOnlyTriggers ? (
                  <Alert>
                    <HugeiconsIcon icon={AlertCircleIcon} size={16} />
                    <AlertDescription>
                      This repository uses local Git — builds can only be
                      triggered manually from the UI or API. Webhook triggers
                      require a connected GitHub or GitLab source.
                    </AlertDescription>
                  </Alert>
                ) : (
                  <>
                    <div className="space-y-2">
                      <FormLabel>Trigger events</FormLabel>
                      <div className="grid grid-cols-1 gap-2 sm:grid-cols-3">
                        {TRIGGER_EVENTS.map((event) => (
                          <label
                            key={event}
                            className="flex items-center gap-2 text-sm"
                          >
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
                            <Input
                              placeholder="main, develop, release/*"
                              {...field}
                            />
                          </FormControl>
                          <FormMessage />
                        </FormItem>
                      )}
                    />
                  </>
                )}

                <div className="space-y-2">
                  <label className="flex items-center gap-2 text-sm">
                    <Checkbox
                      checked={cancelPrevious}
                      onCheckedChange={(checked) => {
                        setCancelPrevious(!!checked)
                        setIsDirty(true)
                      }}
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
              </CardContent>
            </CollapsibleContent>
          </Card>
        </Collapsible>

        {/* Build Commands section */}
        <Collapsible open={commandsOpen} onOpenChange={setCommandsOpen}>
          <Card>
            <CollapsibleTrigger className="w-full cursor-pointer">
              <CardHeader>
                <SectionHeader
                  title="Build Commands"
                  summary={
                    totalCmdCount > 0
                      ? `${totalCmdCount} custom command${totalCmdCount !== 1 ? 's' : ''}`
                      : 'Using defaults'
                  }
                  open={commandsOpen}
                />
              </CardHeader>
            </CollapsibleTrigger>
            <CollapsibleContent>
              <CardContent className="space-y-4">
                <FormField
                  control={form.control}
                  name="enable_customization"
                  render={({ field }) => (
                    <FormItem>
                      <label className="flex items-center gap-2 text-sm font-medium">
                        <Checkbox
                          checked={field.value}
                          onCheckedChange={(checked) =>
                            field.onChange(!!checked)
                          }
                        />
                        Enable custom build commands
                      </label>
                    </FormItem>
                  )}
                />

                {values.enable_customization ? (
                  <>
                    <FormField
                      control={form.control}
                      name="pre_build_commands"
                      render={({ field }) => (
                        <FormItem>
                          <FormLabel>Pre-build commands</FormLabel>
                          <FormControl>
                            <Textarea
                              placeholder={
                                'One command per line\nexample: dart --version'
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
                      name="build_commands"
                      render={({ field }) => (
                        <FormItem>
                          <FormLabel>Extra build commands</FormLabel>
                          <FormControl>
                            <Textarea
                              placeholder={
                                'One command per line\nexample: flutter test'
                              }
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
                              placeholder={
                                'One command per line\nexample: ls -la build/'
                              }
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
                    Using sensible defaults. Enable to add custom pre/build/post
                    commands.
                  </p>
                )}
              </CardContent>
            </CollapsibleContent>
          </Card>
        </Collapsible>

        {/* Platform Build Args section */}
        <Collapsible open={platformArgsOpen} onOpenChange={setPlatformArgsOpen}>
          <Card>
            <CollapsibleTrigger className="w-full cursor-pointer">
              <CardHeader>
                <SectionHeader
                  title="Platform Build Args"
                  summary="Per-platform args + command overrides"
                  open={platformArgsOpen}
                />
              </CardHeader>
            </CollapsibleTrigger>
            <CollapsibleContent>
              <CardContent className="space-y-4">
                <p className="text-xs text-muted-foreground">
                  Each line is appended to the platform's default Flutter build
                  command.
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
                            '--dart-define-from-file=config/dev.json\n--obfuscate'
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
                          placeholder={
                            '--flavor=production\n--build-number=$PROJECT_BUILD_NUMBER'
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
                <p className="text-sm font-medium">
                  Per-platform command override (optional)
                </p>
                <p className="text-xs text-muted-foreground">
                  If set, this command fully replaces the default platform
                  command.
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
              </CardContent>
            </CollapsibleContent>
          </Card>
        </Collapsible>

        {/* Environment Variables section */}
        <Collapsible open={envOpen} onOpenChange={setEnvOpen}>
          <Card>
            <CollapsibleTrigger className="w-full cursor-pointer">
              <CardHeader>
                <SectionHeader
                  title="Environment Variables"
                  summary={
                    envVarCount > 0
                      ? `${envVarCount} env var${envVarCount !== 1 ? 's' : ''} configured`
                      : 'None configured'
                  }
                  open={envOpen}
                />
              </CardHeader>
            </CollapsibleTrigger>
            <CollapsibleContent>
              <CardContent className="space-y-4">
                <FormField
                  control={form.control}
                  name="env_vars"
                  render={({ field }) => (
                    <FormItem>
                      <FormLabel>Environment variables</FormLabel>
                      <FormControl>
                        <Textarea
                          placeholder={
                            'API_BASE_URL=https://api.example.com\nAPP_FLAVOR=dev'
                          }
                          className="font-mono"
                          rows={6}
                          {...field}
                        />
                      </FormControl>
                      <p className="text-xs text-muted-foreground">
                        One KEY=VALUE per line. Available in all build steps.
                      </p>
                      <SetupHint
                        title="Built-in CI variables"
                        items={[
                          <span>
                            Oore also injects <code>PROJECT_ID</code>,{' '}
                            <code>PIPELINE_ID</code>, <code>BUILD_ID</code>,{' '}
                            <code>PROJECT_BUILD_NUMBER</code>,{' '}
                            <code>BUILD_NUMBER</code>, <code>CI=true</code>, and
                            branch/commit values when present.
                          </span>,
                          'Use this section for app-specific values such as API_BASE_URL, APP_FLAVOR, SENTRY_DSN, or feature flags.',
                        ]}
                      />
                      <FormMessage />
                    </FormItem>
                  )}
                />
              </CardContent>
            </CollapsibleContent>
          </Card>
        </Collapsible>

        {/* Artifacts section */}
        <Collapsible open={artifactsOpen} onOpenChange={setArtifactsOpen}>
          <Card>
            <CollapsibleTrigger className="w-full cursor-pointer">
              <CardHeader>
                <SectionHeader
                  title="Artifacts"
                  summary={
                    artifactPatterns.length > 0
                      ? artifactPatterns.join(', ')
                      : 'Using platform defaults'
                  }
                  open={artifactsOpen}
                />
              </CardHeader>
            </CollapsibleTrigger>
            <CollapsibleContent>
              <CardContent className="space-y-4">
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
                          rows={4}
                          {...field}
                        />
                      </FormControl>
                      <FormMessage />
                    </FormItem>
                  )}
                />
              </CardContent>
            </CollapsibleContent>
          </Card>
        </Collapsible>

        {/* Android Signing section */}
        {values.platform_android ? (
          <Collapsible open={signingOpen} onOpenChange={setSigningOpen}>
            <Card>
              <CollapsibleTrigger className="w-full cursor-pointer">
                <CardHeader>
                  <SectionHeader
                    title="Android Signing"
                    summary={
                      values.android_signing_release_enabled ||
                      values.android_signing_debug_enabled
                        ? [
                            values.android_signing_release_enabled
                              ? 'release'
                              : '',
                            values.android_signing_debug_enabled ? 'debug' : '',
                          ]
                            .filter(Boolean)
                            .join(' + ') + ' enabled'
                        : 'Not configured'
                    }
                    open={signingOpen}
                  />
                </CardHeader>
              </CollapsibleTrigger>
              <CollapsibleContent>
                <CardContent className="space-y-4">
                  <SetupHint
                    title="Android project setup"
                    code={ANDROID_GRADLE_SIGNING_SNIPPET}
                    items={[
                      'For standard Flutter release builds, upload the keystore here and Oore prepares the signing files for the runner.',
                      'For custom Gradle flavors or signingConfigs, read the OORE_ANDROID_* environment variables shown below.',
                      'Oore also writes OORE_ANDROID_KEY_PROPERTIES_PATH if your Gradle setup prefers a generated key.properties file.',
                    ]}
                  />
                  <FormField
                    control={form.control}
                    name="android_signing_release_enabled"
                    render={({ field }) => (
                      <FormItem>
                        <label className="flex items-center gap-2 text-sm font-medium">
                          <Checkbox
                            checked={field.value}
                            onCheckedChange={(checked) =>
                              field.onChange(!!checked)
                            }
                          />
                          Enable release signing
                        </label>
                        {signingData ? (
                          <p className="text-xs text-muted-foreground">
                            {signingData.release.has_keystore
                              ? `Stored keystore: ${signingData.release.keystore_filename ?? 'present'}`
                              : 'No stored release keystore'}
                          </p>
                        ) : null}
                        {!values.android_signing_release_enabled &&
                        !values.android_signing_debug_enabled ? (
                          <p className="text-xs text-muted-foreground">
                            Signing is optional for debug builds. Enable it when
                            you're ready to distribute release builds.
                          </p>
                        ) : null}
                      </FormItem>
                    )}
                  />

                  {values.android_signing_release_enabled ? (
                    <div className="grid gap-3 border p-3">
                      <FormItem>
                        <FormLabel>Release keystore (.jks)</FormLabel>
                        <FormControl>
                          <Input
                            type="file"
                            accept=".jks,.keystore"
                            onChange={(event) => {
                              const file = event.target.files?.[0] ?? null
                              setReleaseKeystoreFile(file)
                            }}
                          />
                        </FormControl>
                        <p className="text-xs text-muted-foreground">
                          {releaseKeystoreFile
                            ? `Selected: ${releaseKeystoreFile.name}`
                            : signingData?.release.has_keystore
                              ? 'Keep existing keystore or select a new file'
                              : 'Select a JKS/keystore file'}
                        </p>
                      </FormItem>

                      <FormField
                        control={form.control}
                        name="android_signing_release_key_alias"
                        render={({ field }) => (
                          <FormItem>
                            <FormLabel>Release key alias</FormLabel>
                            <FormControl>
                              <Input
                                placeholder="upload"
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
                        name="android_signing_release_store_password"
                        render={({ field }) => (
                          <FormItem>
                            <FormLabel>Release store password</FormLabel>
                            <FormControl>
                              <Input
                                type="password"
                                placeholder={
                                  signingData?.release.has_store_password
                                    ? 'Leave empty to keep existing password'
                                    : ''
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
                        name="android_signing_release_key_password"
                        render={({ field }) => (
                          <FormItem>
                            <FormLabel>Release key password</FormLabel>
                            <FormControl>
                              <Input
                                type="password"
                                placeholder={
                                  signingData?.release.has_key_password
                                    ? 'Leave empty to keep existing password'
                                    : ''
                                }
                                className="font-mono"
                                {...field}
                              />
                            </FormControl>
                            <FormMessage />
                          </FormItem>
                        )}
                      />
                    </div>
                  ) : null}

                  <Separator />

                  <FormField
                    control={form.control}
                    name="android_signing_debug_enabled"
                    render={({ field }) => (
                      <FormItem>
                        <label className="flex items-center gap-2 text-sm font-medium">
                          <Checkbox
                            checked={field.value}
                            onCheckedChange={(checked) =>
                              field.onChange(!!checked)
                            }
                          />
                          Enable debug signing
                        </label>
                        {signingData ? (
                          <p className="text-xs text-muted-foreground">
                            {signingData.debug.has_keystore
                              ? `Stored keystore: ${signingData.debug.keystore_filename ?? 'present'}`
                              : 'No stored debug keystore'}
                          </p>
                        ) : null}
                      </FormItem>
                    )}
                  />

                  {values.android_signing_debug_enabled ? (
                    <div className="grid gap-3 border p-3">
                      <FormItem>
                        <FormLabel>Debug keystore (.jks)</FormLabel>
                        <FormControl>
                          <Input
                            type="file"
                            accept=".jks,.keystore"
                            onChange={(event) => {
                              const file = event.target.files?.[0] ?? null
                              setDebugKeystoreFile(file)
                            }}
                          />
                        </FormControl>
                        <p className="text-xs text-muted-foreground">
                          {debugKeystoreFile
                            ? `Selected: ${debugKeystoreFile.name}`
                            : signingData?.debug.has_keystore
                              ? 'Keep existing keystore or select a new file'
                              : 'Select a JKS/keystore file'}
                        </p>
                      </FormItem>

                      <FormField
                        control={form.control}
                        name="android_signing_debug_key_alias"
                        render={({ field }) => (
                          <FormItem>
                            <FormLabel>Debug key alias</FormLabel>
                            <FormControl>
                              <Input
                                placeholder="androiddebugkey"
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
                        name="android_signing_debug_store_password"
                        render={({ field }) => (
                          <FormItem>
                            <FormLabel>Debug store password</FormLabel>
                            <FormControl>
                              <Input
                                type="password"
                                placeholder={
                                  signingData?.debug.has_store_password
                                    ? 'Leave empty to keep existing password'
                                    : ''
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
                        name="android_signing_debug_key_password"
                        render={({ field }) => (
                          <FormItem>
                            <FormLabel>Debug key password</FormLabel>
                            <FormControl>
                              <Input
                                type="password"
                                placeholder={
                                  signingData?.debug.has_key_password
                                    ? 'Leave empty to keep existing password'
                                    : ''
                                }
                                className="font-mono"
                                {...field}
                              />
                            </FormControl>
                            <FormMessage />
                          </FormItem>
                        )}
                      />
                    </div>
                  ) : null}
                </CardContent>
              </CollapsibleContent>
            </Card>
          </Collapsible>
        ) : null}

        {/* iOS Signing section */}
        {values.platform_ios ? (
          <Collapsible open={iosSigningOpen} onOpenChange={setIosSigningOpen}>
            <Card>
              <CollapsibleTrigger className="w-full cursor-pointer">
                <CardHeader>
                  <SectionHeader
                    title="iOS Signing"
                    summary={
                      values.ios_signing_enabled
                        ? `${iosSigningMode} mode`
                        : 'Not configured'
                    }
                    open={iosSigningOpen}
                  />
                </CardHeader>
              </CollapsibleTrigger>
              <CollapsibleContent>
                <CardContent className="space-y-4">
                  <SetupHint
                    title="iOS project setup"
                    items={[
                      'Manual mode uses your uploaded .p12 certificate and .mobileprovision files.',
                      'API mode uses App Store Connect credentials to sync signing assets; hybrid mode combines API sync with a manually uploaded certificate.',
                      'During a build, Oore installs profiles, creates a temporary keychain, and pins CODE_SIGN_IDENTITY when a signing identity is available.',
                      'Keep bundle identifiers aligned with the Xcode targets you expect to sign.',
                    ]}
                  />
                  <FormField
                    control={form.control}
                    name="ios_signing_enabled"
                    render={({ field }) => (
                      <FormItem>
                        <label className="flex items-center gap-2 text-sm font-medium">
                          <Checkbox
                            checked={field.value}
                            onCheckedChange={(checked) =>
                              field.onChange(!!checked)
                            }
                          />
                          Enable iOS ad hoc signing
                        </label>
                        {iosSigningData ? (
                          <p className="text-xs text-muted-foreground">
                            Stored: mode {iosSigningData.mode}, p12{' '}
                            {iosSigningData.has_p12 ? 'present' : 'missing'},
                            API key{' '}
                            {iosSigningData.has_api_key ? 'present' : 'missing'}
                          </p>
                        ) : null}
                        {!values.ios_signing_enabled ? (
                          <p className="text-xs text-muted-foreground">
                            Required for installing on physical iOS devices.
                            You'll need a distribution certificate (.p12) and
                            provisioning profiles.
                          </p>
                        ) : null}
                        <FormMessage />
                      </FormItem>
                    )}
                  />

                  {values.ios_signing_enabled ? (
                    <>
                      <FormField
                        control={form.control}
                        name="ios_signing_mode"
                        render={({ field }) => (
                          <FormItem>
                            <FormLabel>Signing mode</FormLabel>
                            <Select
                              value={field.value}
                              onValueChange={field.onChange}
                              items={SIGNING_MODES}
                            >
                              <FormControl>
                                <SelectTrigger>
                                  <SelectValue />
                                </SelectTrigger>
                              </FormControl>
                              <SelectContent>
                                {Object.entries(SIGNING_MODES).map(
                                  ([key, value]) => (
                                    <SelectItem key={key} value={key}>
                                      {value}
                                    </SelectItem>
                                  ),
                                )}
                              </SelectContent>
                            </Select>
                            <FormMessage />
                          </FormItem>
                        )}
                      />

                      <FormField
                        control={form.control}
                        name="ios_signing_team_id"
                        render={({ field }) => (
                          <FormItem>
                            <FormLabel>Apple Team ID</FormLabel>
                            <FormControl>
                              <Input
                                placeholder="TEAM1234"
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
                        name="ios_signing_bundle_ids"
                        render={({ field }) => (
                          <FormItem>
                            <FormLabel>Bundle identifiers</FormLabel>
                            <FormControl>
                              <Textarea
                                placeholder={
                                  'com.example.app\ncom.example.app.share-extension'
                                }
                                className="font-mono"
                                rows={3}
                                {...field}
                              />
                            </FormControl>
                            <p className="text-xs text-muted-foreground">
                              Main bundle first, then optional extension bundle
                              IDs (one per line or comma-separated).
                            </p>
                            <FormMessage />
                          </FormItem>
                        )}
                      />

                      {iosSigningMode === 'manual' ||
                      iosSigningMode === 'hybrid' ? (
                        <div className="grid gap-3 border p-3">
                          <FormItem>
                            <FormLabel>
                              Distribution certificate (.p12)
                            </FormLabel>
                            <FormControl>
                              <Input
                                type="file"
                                accept=".p12"
                                onChange={(event) => {
                                  const file = event.target.files?.[0] ?? null
                                  setIosP12File(file)
                                }}
                              />
                            </FormControl>
                            <p className="text-xs text-muted-foreground">
                              {iosP12File
                                ? `Selected: ${iosP12File.name}`
                                : iosSigningData?.has_p12
                                  ? `Stored p12: ${iosSigningData.p12_filename ?? 'present'}`
                                  : 'Select a .p12 certificate file'}
                            </p>
                          </FormItem>

                          <FormField
                            control={form.control}
                            name="ios_signing_p12_password"
                            render={({ field }) => (
                              <FormItem>
                                <FormLabel>P12 password</FormLabel>
                                <FormControl>
                                  <Input
                                    type="password"
                                    placeholder={
                                      iosSigningData?.has_p12_password
                                        ? 'Leave empty to keep stored password'
                                        : ''
                                    }
                                    className="font-mono"
                                    {...field}
                                  />
                                </FormControl>
                                <FormMessage />
                              </FormItem>
                            )}
                          />
                        </div>
                      ) : null}

                      {iosSigningMode === 'api' ||
                      iosSigningMode === 'hybrid' ? (
                        <div className="grid gap-3 border p-3">
                          <FormField
                            control={form.control}
                            name="ios_signing_api_key_id"
                            render={({ field }) => (
                              <FormItem>
                                <FormLabel>API key ID</FormLabel>
                                <FormControl>
                                  <Input
                                    placeholder="ABC123XYZ"
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
                            name="ios_signing_api_issuer_id"
                            render={({ field }) => (
                              <FormItem>
                                <FormLabel>API issuer ID (UUID)</FormLabel>
                                <FormControl>
                                  <Input
                                    placeholder="00000000-0000-0000-0000-000000000000"
                                    className="font-mono"
                                    {...field}
                                  />
                                </FormControl>
                                <FormMessage />
                              </FormItem>
                            )}
                          />
                          <FormItem>
                            <FormLabel>App Store Connect key (.p8)</FormLabel>
                            <FormControl>
                              <Input
                                type="file"
                                accept=".p8,text/plain"
                                onChange={(event) => {
                                  const file = event.target.files?.[0] ?? null
                                  setIosApiKeyFile(file)
                                }}
                              />
                            </FormControl>
                            <p className="text-xs text-muted-foreground">
                              {iosApiKeyFile
                                ? `Selected: ${iosApiKeyFile.name}`
                                : iosSigningData?.has_api_key
                                  ? `Stored key: ${iosSigningData.api_key_id ?? 'present'}`
                                  : 'Upload App Store Connect private key file (.p8)'}
                            </p>
                          </FormItem>
                        </div>
                      ) : null}

                      {iosSigningMode === 'manual' ||
                      iosSigningMode === 'hybrid' ? (
                        <div className="space-y-3 border p-3">
                          <p className="text-sm font-medium">
                            Provisioning profiles by bundle ID
                          </p>
                          {iosBundleIds.length === 0 ? (
                            <p className="text-xs text-muted-foreground">
                              Add at least one bundle ID to attach provisioning
                              profiles.
                            </p>
                          ) : (
                            <div className="space-y-3">
                              {iosBundleIds.map((bundleId) => {
                                const existing =
                                  iosProfilesByBundle.get(bundleId)
                                const selectedFile = iosProfileFiles[bundleId]
                                return (
                                  <FormItem key={bundleId}>
                                    <FormLabel className="font-mono text-xs">
                                      {bundleId}
                                    </FormLabel>
                                    <FormControl>
                                      <Input
                                        type="file"
                                        accept=".mobileprovision"
                                        onChange={(event) => {
                                          const file =
                                            event.target.files?.[0] ?? null
                                          setIosProfileFiles((prev) => ({
                                            ...prev,
                                            [bundleId]: file,
                                          }))
                                        }}
                                      />
                                    </FormControl>
                                    <p className="text-xs text-muted-foreground">
                                      {selectedFile
                                        ? `Selected: ${selectedFile.name}`
                                        : existing?.has_profile
                                          ? `Stored profile: ${existing.profile_filename ?? existing.profile_name ?? 'present'}`
                                          : 'Upload .mobileprovision for this bundle ID'}
                                    </p>
                                  </FormItem>
                                )
                              })}
                            </div>
                          )}
                        </div>
                      ) : null}
                    </>
                  ) : null}
                </CardContent>
              </CollapsibleContent>
            </Card>
          </Collapsible>
        ) : null}

        {validationErrors.length > 0 ? (
          <div>
            <Alert variant="destructive">
              <HugeiconsIcon icon={AlertCircleIcon} size={16} />
              <AlertDescription>
                <ul className="list-disc space-y-1 pl-4">
                  {validationErrors.map((err, index) => (
                    <li key={index}>{err}</li>
                  ))}
                </ul>
              </AlertDescription>
            </Alert>
          </div>
        ) : null}

        {signingError ? (
          <Alert variant="destructive">
            <HugeiconsIcon icon={AlertCircleIcon} size={16} />
            <AlertDescription>
              Pipeline creation completed, but {retrySigning} signing failed:{' '}
              {signingError}. Fix the signing fields below and retry only
              signing.
            </AlertDescription>
          </Alert>
        ) : null}

        {values.ios_signing_enabled &&
          (values.ios_signing_mode === 'api' ||
            values.ios_signing_mode === 'hybrid') &&
          children}

        {/* Sticky action bar */}
        <div className="sticky bottom-0 z-30 -mx-6 border-t bg-surface/95 backdrop-blur supports-backdrop-filter:bg-surface/60">
          <div className="flex items-center justify-end gap-3 px-6 py-3">
            <Button type="button" variant="outline" onClick={onCancel}>
              Cancel
            </Button>
            <Button type="button" variant="outline" onClick={handleValidate}>
              Validate
            </Button>
            <Button
              type="button"
              disabled={isPending || readOnly}
              title={readOnly ? readOnlyReason : undefined}
              onClick={() => {
                void form.handleSubmit(handleFormSubmit)()
              }}
            >
              {isPending ? (
                <>
                  <Spinner className="size-4" />
                  Saving...
                </>
              ) : readOnly ? (
                'Demo is read-only'
              ) : (
                submitLabel
              )}
            </Button>
          </div>
        </div>
      </form>
    </Form>
  )
}
