import { useEffect, useState } from 'react'
import { useForm } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  AlertCircleIcon,
  ArrowDown01Icon,
  ArrowUp01Icon,
} from '@hugeicons/core-free-icons'

import type { PipelineFormValues } from '@/lib/pipeline-schema'
import { Button } from '@/components/ui/button'
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
  ) => Promise<void>
  onCancel: () => void
  submitLabel: string
  isPending: boolean
  validationErrors?: Array<string>
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
}

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
    <div className="flex w-full items-center justify-between py-3">
      <div className="flex items-center gap-2">
        <span className="text-sm font-medium">{title}</span>
        {errorCount && errorCount > 0 ? (
          <Badge variant="destructive" className="text-[10px]">
            {errorCount} {errorCount === 1 ? 'error' : 'errors'}
          </Badge>
        ) : null}
      </div>
      <div className="flex items-center gap-2">
        {!open && summary ? (
          <span className="text-xs text-muted-foreground">{summary}</span>
        ) : null}
        <HugeiconsIcon
          icon={open ? ArrowUp01Icon : ArrowDown01Icon}
          size={14}
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
  signingData,
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
  const [isDirty, setIsDirty] = useState(false)

  const [configOpen, setConfigOpen] = useState(true)
  const [triggersOpen, setTriggersOpen] = useState(true)
  const [commandsOpen, setCommandsOpen] = useState(false)
  const [platformArgsOpen, setPlatformArgsOpen] = useState(false)
  const [envOpen, setEnvOpen] = useState(false)
  const [artifactsOpen, setArtifactsOpen] = useState(false)
  const [signingOpen, setSigningOpen] = useState(false)

  useEffect(() => {
    const subscription = form.watch(() => setIsDirty(true))
    return () => subscription.unsubscribe()
  }, [form])

  useEffect(() => {
    function handleBeforeUnload(e: BeforeUnloadEvent) {
      if (isDirty) {
        e.preventDefault()
      }
    }
    window.addEventListener('beforeunload', handleBeforeUnload)
    return () => window.removeEventListener('beforeunload', handleBeforeUnload)
  }, [isDirty])

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

  return (
    <Form {...form}>
      <form
        onSubmit={form.handleSubmit(handleFormSubmit)}
        className="space-y-0 pb-24"
      >
        {/* Name - always visible */}
        <div className="border-b px-1 py-4">
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
        </div>

        {/* Configuration section */}
        <Collapsible open={configOpen} onOpenChange={setConfigOpen}>
          <div className="border-b px-1">
            <CollapsibleTrigger className="w-full cursor-pointer">
              <SectionHeader
                title="Configuration"
                summary={`${platforms.length} platform${platforms.length !== 1 ? 's' : ''}, ${configMode === 'explicit' ? 'explicit config' : 'auto-detect'}`}
                open={configOpen}
              />
            </CollapsibleTrigger>
            <CollapsibleContent>
              <div className="space-y-4 pb-4">
                <FormField
                  control={form.control}
                  name="config_mode"
                  render={({ field }) => (
                    <FormItem>
                      <FormLabel>Config source</FormLabel>
                      <Select
                        value={field.value}
                        onValueChange={field.onChange}
                      >
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
                  <div className="space-y-1 rounded-md border p-3">
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
              </div>
            </CollapsibleContent>
          </div>
        </Collapsible>

        {/* Triggers section */}
        <Collapsible open={triggersOpen} onOpenChange={setTriggersOpen}>
          <div className="border-b px-1">
            <CollapsibleTrigger className="w-full cursor-pointer">
              <SectionHeader
                title="Triggers"
                summary={`${selectedEvents.length} event${selectedEvents.length !== 1 ? 's' : ''}, cancel previous: ${cancelPrevious ? 'on' : 'off'}`}
                open={triggersOpen}
              />
            </CollapsibleTrigger>
            <CollapsibleContent>
              <div className="space-y-4 pb-4">
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
              </div>
            </CollapsibleContent>
          </div>
        </Collapsible>

        {/* Build Commands section */}
        <Collapsible open={commandsOpen} onOpenChange={setCommandsOpen}>
          <div className="border-b px-1">
            <CollapsibleTrigger className="w-full cursor-pointer">
              <SectionHeader
                title="Build Commands"
                summary={
                  totalCmdCount > 0
                    ? `${totalCmdCount} custom command${totalCmdCount !== 1 ? 's' : ''}`
                    : 'Using defaults'
                }
                open={commandsOpen}
              />
            </CollapsibleTrigger>
            <CollapsibleContent>
              <div className="space-y-4 pb-4">
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
              </div>
            </CollapsibleContent>
          </div>
        </Collapsible>

        {/* Platform Build Args section */}
        <Collapsible open={platformArgsOpen} onOpenChange={setPlatformArgsOpen}>
          <div className="border-b px-1">
            <CollapsibleTrigger className="w-full cursor-pointer">
              <SectionHeader
                title="Platform Build Args"
                summary="Per-platform args + command overrides"
                open={platformArgsOpen}
              />
            </CollapsibleTrigger>
            <CollapsibleContent>
              <div className="space-y-4 pb-4">
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
              </div>
            </CollapsibleContent>
          </div>
        </Collapsible>

        {/* Environment Variables section */}
        <Collapsible open={envOpen} onOpenChange={setEnvOpen}>
          <div className="border-b px-1">
            <CollapsibleTrigger className="w-full cursor-pointer">
              <SectionHeader
                title="Environment Variables"
                summary={
                  envVarCount > 0
                    ? `${envVarCount} env var${envVarCount !== 1 ? 's' : ''} configured`
                    : 'None configured'
                }
                open={envOpen}
              />
            </CollapsibleTrigger>
            <CollapsibleContent>
              <div className="space-y-4 pb-4">
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
                      <FormMessage />
                    </FormItem>
                  )}
                />
              </div>
            </CollapsibleContent>
          </div>
        </Collapsible>

        {/* Artifacts section */}
        <Collapsible open={artifactsOpen} onOpenChange={setArtifactsOpen}>
          <div className="border-b px-1">
            <CollapsibleTrigger className="w-full cursor-pointer">
              <SectionHeader
                title="Artifacts"
                summary={
                  artifactPatterns.length > 0
                    ? artifactPatterns.join(', ')
                    : 'Using platform defaults'
                }
                open={artifactsOpen}
              />
            </CollapsibleTrigger>
            <CollapsibleContent>
              <div className="space-y-4 pb-4">
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
              </div>
            </CollapsibleContent>
          </div>
        </Collapsible>

        {/* Android Signing section */}
        <Collapsible open={signingOpen} onOpenChange={setSigningOpen}>
          <div className="border-b px-1">
            <CollapsibleTrigger className="w-full cursor-pointer">
              <SectionHeader
                title="Android Signing"
                summary={
                  values.android_signing_release_enabled ||
                  values.android_signing_debug_enabled
                    ? [
                        values.android_signing_release_enabled ? 'release' : '',
                        values.android_signing_debug_enabled ? 'debug' : '',
                      ]
                        .filter(Boolean)
                        .join(' + ') + ' enabled'
                    : 'Not configured'
                }
                open={signingOpen}
              />
            </CollapsibleTrigger>
            <CollapsibleContent>
              <div className="space-y-4 pb-4">
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
                    </FormItem>
                  )}
                />

                {values.android_signing_release_enabled ? (
                  <div className="grid gap-3 rounded-md border p-3">
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
                  <div className="grid gap-3 rounded-md border p-3">
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
              </div>
            </CollapsibleContent>
          </div>
        </Collapsible>

        {validationErrors.length > 0 ? (
          <div className="px-1 pt-4">
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

        {/* Sticky action bar */}
        <div className="fixed inset-x-0 bottom-0 z-50 border-t bg-background/95 backdrop-blur supports-[backdrop-filter]:bg-background/60">
          <div className="mx-auto flex max-w-4xl items-center justify-end gap-3 px-6 py-3">
            <Button type="button" variant="outline" onClick={onCancel}>
              Cancel
            </Button>
            <Button type="button" variant="outline" onClick={handleValidate}>
              Validate
            </Button>
            <Button
              type="button"
              disabled={isPending}
              onClick={() => {
                void form.handleSubmit(handleFormSubmit)()
              }}
            >
              {isPending ? (
                <>
                  <Spinner className="size-4" />
                  Saving...
                </>
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
