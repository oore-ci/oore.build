import type {
  BuildPlatform,
  CreatePipelineRequest,
  Pipeline,
  PipelineExecutionConfig,
  RepositoryWorkflowExecutionPreview,
  RepositoryWorkflowPreview,
  SizeBudget,
  TriggerConfig,
  UpdatePipelineRequest,
} from '@oore/client/models'
import type { PipelineFormValues } from '@/lib/pipeline-schema'

export function buildNumberOffsetField(
  config?: PipelineExecutionConfig | RepositoryWorkflowExecutionPreview | null,
): string {
  return config?.build_number_offset ? String(config.build_number_offset) : ''
}

export function runsOnField(
  config?: PipelineExecutionConfig | RepositoryWorkflowExecutionPreview | null,
): string {
  return config?.runs_on?.join(', ') ?? ''
}

export function parseMultiline(raw?: string): Array<string> {
  if (!raw) return []
  return raw
    .split('\n')
    .map((line) => line.trim())
    .filter(Boolean)
}

export function parseCsv(raw?: string): Array<string> {
  if (!raw) return []
  return raw
    .split(',')
    .map((value) => value.trim())
    .filter(Boolean)
}

export function parseEnvVars(
  raw?: string,
): Array<{ key: string; value: string }> {
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

export function selectedPlatforms(
  data: PipelineFormValues,
): Array<BuildPlatform> {
  const platforms: Array<BuildPlatform> = []
  if (data.platform_android) platforms.push('android')
  if (data.platform_ios) platforms.push('ios')
  if (data.platform_macos) platforms.push('macos')
  return platforms
}

export function applyArgs(base: string, args: Array<string>): string {
  if (args.length === 0) return base
  return `${base} ${args.join(' ')}`
}

/**
 * The platform build commands a run executes, mirroring the runner. With
 * `playBundle` (Google Play upload on), the default Android build also makes
 * an App Bundle, because Play does not accept APKs.
 */
export function previewPlatformCommands(
  data: PipelineFormValues,
  { playBundle = false }: { playBundle?: boolean } = {},
): Array<string> {
  const platforms = selectedPlatforms(data)
  const commands: Array<string> = []
  for (const platform of platforms) {
    if (platform === 'android') {
      const override = data.android_command_override?.trim()
      const args = parseMultiline(data.android_build_args)
      commands.push(override || applyArgs('flutter build apk --release', args))
      if (playBundle && !override) {
        commands.push(
          applyArgs(
            'flutter build appbundle --release',
            // `--split-per-abi` is APK-only; the runner drops it too.
            args.filter((arg) => arg !== '--split-per-abi'),
          ),
        )
      }
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
          applyArgs(
            'flutter build macos --release',
            parseMultiline(data.macos_build_args),
          ),
      )
    }
  }
  return commands.filter(Boolean)
}

/** Whether an Android build command produces an App Bundle. */
export function buildsAppBundle(command: string): boolean {
  return /\bflutter\s+build\s+appbundle\b/.test(command)
}

export function defaultArtifactPatterns(
  platforms: Array<BuildPlatform>,
): Array<string> {
  const patterns = new Set<string>()
  if (platforms.includes('android'))
    patterns.add('build/app/outputs/flutter-apk/*.apk')
  if (platforms.includes('ios')) patterns.add('build/ios/ipa/*.ipa')
  if (platforms.includes('macos'))
    patterns.add('build/macos/Build/Products/Release/*.app')
  if (patterns.size === 0) patterns.add('build/app/outputs/flutter-apk/*.apk')
  return [...patterns]
}

export function executionConfigFromForm(
  data: PipelineFormValues,
): PipelineExecutionConfig {
  const platforms = selectedPlatforms(data)
  const artifactPatterns = parseMultiline(data.artifact_patterns)

  return {
    platforms,
    flutter_version: trimToUndefined(data.flutter_version),
    commands: data.enable_customization
      ? {
          pre_build: parseMultiline(data.pre_build_commands),
          build: parseMultiline(data.build_commands),
          post_build: parseMultiline(data.post_build_commands),
        }
      : { pre_build: [], build: [], post_build: [] },
    platform_build_args: {
      android: parseMultiline(data.android_build_args),
      ios: parseMultiline(data.ios_build_args),
      macos: parseMultiline(data.macos_build_args),
    },
    platform_commands: {
      android: trimToUndefined(data.android_command_override),
      ios: trimToUndefined(data.ios_command_override),
      macos: trimToUndefined(data.macos_command_override),
    },
    env: parseEnvVars(data.env_vars),
    artifact_patterns:
      artifactPatterns.length > 0
        ? artifactPatterns
        : defaultArtifactPatterns(platforms),
    build_number_offset: Number(data.build_number_offset?.trim() || 0),
    isolation: data.isolation,
    runs_on: parseRunnerLabels(data.runs_on),
  }
}

/** Labels are case-insensitive on the server; send them the way it stores them. */
export function parseRunnerLabels(raw?: string): Array<string> {
  return [...new Set(parseCsv(raw).map((label) => label.toLowerCase()))].sort()
}

/** Tag globs only apply with the tag push event; an empty event list means every event. */
export function tagsApply(events: Array<string>): boolean {
  return events.length === 0 || events.includes('tag_push')
}

/** Path filters only apply to push and pull request events. */
export function pathsApply(events: Array<string>): boolean {
  return (
    events.length === 0 ||
    events.includes('push') ||
    events.includes('pull_request')
  )
}

export function triggerConfigFromForm(
  data: PipelineFormValues,
  manualOnlyTriggers: boolean,
): TriggerConfig {
  const schedules = data.schedules.map(({ branch, cron }) => ({
    cron: cron.trim(),
    branch: trimToUndefined(branch),
  }))
  if (manualOnlyTriggers) return { events: [], branches: [], schedules }
  const events = data.trigger_events
  return {
    events,
    branches: parseCsv(data.branches),
    tags: tagsApply(events) ? parseCsv(data.tags) : [],
    paths: pathsApply(events) ? parseMultiline(data.paths) : [],
    paths_ignore: pathsApply(events) ? parseMultiline(data.paths_ignore) : [],
    schedules,
  }
}

const MEBIBYTE = 1024 * 1024

/**
 * A byte count as MB for the form. Seven decimals round-trip any byte count
 * exactly, and whole sizes such as 25 MB still show as `25`.
 */
export function bytesToMegabytes(bytes?: number | null): string {
  if (bytes === undefined || bytes === null) return ''
  return String(Number((bytes / MEBIBYTE).toFixed(7)))
}

function megabytesToBytes(value?: string): number | undefined {
  const trimmed = value?.trim()
  return trimmed ? Math.round(Number(trimmed) * MEBIBYTE) : undefined
}

function optionalNumber(value?: string): number | undefined {
  const trimmed = value?.trim()
  return trimmed ? Number(trimmed) : undefined
}

export function sizeBudgetFromForm(data: PipelineFormValues): SizeBudget {
  return {
    mode: data.size_budget_mode,
    rules: data.size_budget_rules.map((rule) => ({
      kind: rule.kind.trim(),
      max_bytes: megabytesToBytes(rule.max_mb),
      max_increase_bytes: megabytesToBytes(rule.max_increase_mb),
      max_increase_percent: optionalNumber(rule.max_increase_percent),
    })),
  }
}

export function sizeBudgetFormFields(
  budget?: SizeBudget | null,
): Pick<PipelineFormValues, 'size_budget_mode' | 'size_budget_rules'> {
  return {
    size_budget_mode: budget?.mode ?? 'warn',
    size_budget_rules: (budget?.rules ?? []).map((rule) => ({
      kind: rule.kind,
      max_mb: bytesToMegabytes(rule.max_bytes),
      max_increase_mb: bytesToMegabytes(rule.max_increase_bytes),
      max_increase_percent:
        rule.max_increase_percent === undefined ||
        rule.max_increase_percent === null
          ? ''
          : String(rule.max_increase_percent),
    })),
  }
}

export function pipelineRequestFromForm(
  data: PipelineFormValues,
  {
    canChangeApproval,
    manualOnlyTriggers,
  }: {
    /** Only maintainers may send an approval policy; others leave it untouched. */
    canChangeApproval: boolean
    manualOnlyTriggers: boolean
  },
): CreatePipelineRequest {
  return {
    name: data.name.trim(),
    config_path:
      data.config_mode === 'explicit' ? data.config_path?.trim() : '.oore.yaml',
    config_path_explicit: data.config_mode === 'explicit',
    execution_config: executionConfigFromForm(data),
    trigger_config: triggerConfigFromForm(data, manualOnlyTriggers),
    concurrency: {
      cancel_previous: data.cancel_previous,
      max_concurrent: data.max_concurrent
        ? Number(data.max_concurrent)
        : undefined,
    },
    timeout_minutes: Number(data.timeout_minutes),
    approval: canChangeApproval
      ? {
          required: data.approval_required,
          events: data.approval_required ? data.approval_events : [],
        }
      : undefined,
    size_budget: sizeBudgetFromForm(data),
    workflow: trimToUndefined(data.workflow),
  }
}

/**
 * An update request from the form. The workflow is sent only when it
 * changed, so saving never clears one the selector did not show.
 */
export function pipelineUpdateFromForm(
  data: PipelineFormValues,
  savedWorkflow: string | null | undefined,
  options: { canChangeApproval: boolean; manualOnlyTriggers: boolean },
): UpdatePipelineRequest {
  const { workflow, ...request } = pipelineRequestFromForm(data, options)
  return (workflow ?? null) === (savedWorkflow ?? null)
    ? request
    : { ...request, workflow: workflow ?? null }
}

/**
 * The discovered file a pipeline builds from: its explicit config path, or
 * the first of `.oore.yaml` and `.oore.yml` that exists, as the runner
 * picks it.
 */
export function pipelineRepositoryFile(
  files: Array<RepositoryWorkflowPreview>,
  pipeline: { config_path: string; config_path_explicit: boolean },
): RepositoryWorkflowPreview | undefined {
  const candidates = pipeline.config_path_explicit
    ? [pipeline.config_path.trim()]
    : ['.oore.yaml', '.oore.yml']
  for (const path of candidates) {
    const file = files.find((candidate) => candidate.path === path)
    if (file) return file
  }
  return undefined
}

export function trimToUndefined(value?: string | null): string | undefined {
  const trimmed = value?.trim()
  return trimmed ? trimmed : undefined
}

export async function fileToBase64(file: File): Promise<string> {
  const arrayBuffer = await file.arrayBuffer()
  const bytes = new Uint8Array(arrayBuffer)
  let binary = ''
  for (const b of bytes) {
    binary += String.fromCharCode(b)
  }
  return btoa(binary)
}

export function parseBundleIdsInput(raw?: string): Array<string> {
  if (!raw) return []
  const seen = new Set<string>()
  const out: Array<string> = []
  for (const part of raw.split(/[\n,]/g)) {
    const trimmed = part.trim()
    if (!trimmed || seen.has(trimmed)) continue
    seen.add(trimmed)
    out.push(trimmed)
  }
  return out
}

export function hasSigningFileChanges(
  files: Array<File | null>,
  profiles: Record<string, File | null>,
): boolean {
  return (
    files.some((file) => file !== null) ||
    Object.values(profiles).some((file) => file !== null)
  )
}

export function toMultiline(values: Array<string>): string {
  return values.join('\n')
}

export function hasCustomFallback(pipeline: Pipeline): boolean {
  const commands = pipeline.execution_config.commands
  if (commands.pre_build.length > 0) return true
  if (commands.build.length > 0) return true
  if (commands.post_build.length > 0) return true
  if ((pipeline.execution_config.env?.length ?? 0) > 0) return true

  const args = pipeline.execution_config.platform_build_args
  if ((args?.android.length ?? 0) > 0) return true
  if ((args?.ios.length ?? 0) > 0) return true
  if ((args?.macos.length ?? 0) > 0) return true

  const overrides = pipeline.execution_config.platform_commands
  if (overrides?.android?.trim()) return true
  if (overrides?.ios?.trim()) return true
  if (overrides?.macos?.trim()) return true

  const defaults = defaultArtifactPatterns(pipeline.execution_config.platforms)
  const current = [...pipeline.execution_config.artifact_patterns].sort()
  const expected = [...defaults].sort()

  return JSON.stringify(current) !== JSON.stringify(expected)
}
