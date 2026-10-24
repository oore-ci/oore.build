import type {
  ExportPipelineYamlRequest,
  ImportPipelineYamlRequest,
  ImportPipelineYamlResponse,
  PipelineExecutionConfig,
  TriggerConfig,
} from '@oore/client/models'
import type { PipelineFormValues } from '@/lib/pipeline-schema'
import {
  buildNumberOffsetField,
  runsOnField,
  toMultiline,
} from '@/lib/pipeline-form-utils'

/**
 * The import request for an edited file: the text, plus the settings the
 * export was made from. The daemon keeps whatever of those the file did not
 * change, and compares the file's `publish` with the Publish settings.
 */
export function yamlImportRequest(
  yaml: string,
  exportRequest: ExportPipelineYamlRequest,
): ImportPipelineYamlRequest {
  return { yaml, ...exportRequest }
}

/** The form fields that hold execution and trigger settings. */
const IMPORTED_FIELDS = [
  'platform_android',
  'platform_ios',
  'platform_macos',
  'flutter_version',
  'build_number_offset',
  'runs_on',
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
  'isolation',
  'trigger_events',
  'branches',
  'tags',
  'paths',
  'paths_ignore',
  'schedules',
] as const satisfies ReadonlyArray<keyof PipelineFormValues>

export type ImportedFieldName = (typeof IMPORTED_FIELDS)[number]
export type ImportedFormFields = Partial<
  Pick<PipelineFormValues, ImportedFieldName>
>

function setField<K extends ImportedFieldName>(
  target: ImportedFormFields,
  name: K,
  value: ImportedFormFields[K],
) {
  target[name] = value
}

/** Visit each field an import sets, in a fixed order. */
export function forEachField(
  fields: ImportedFormFields,
  visit: <K extends ImportedFieldName>(
    name: K,
    value: NonNullable<ImportedFormFields[K]>,
  ) => void,
) {
  for (const name of IMPORTED_FIELDS) {
    const value = fields[name]
    if (value !== undefined) visit(name, value)
  }
}

/** Form fields for imported execution settings, as the edit route fills them. */
export function executionFormFields(
  config: PipelineExecutionConfig,
  current: PipelineFormValues,
): ImportedFormFields {
  const commands = config.commands
  const hasCommands =
    commands.pre_build.length +
      commands.build.length +
      commands.post_build.length >
    0
  return {
    platform_android: config.platforms.includes('android'),
    platform_ios: config.platforms.includes('ios'),
    platform_macos: config.platforms.includes('macos'),
    flutter_version: config.flutter_version ?? '',
    build_number_offset: buildNumberOffsetField(config),
    runs_on: runsOnField(config),
    // Commands only reach the settings with customization on.
    enable_customization: current.enable_customization || hasCommands,
    pre_build_commands: toMultiline(commands.pre_build),
    build_commands: toMultiline(commands.build),
    post_build_commands: toMultiline(commands.post_build),
    android_build_args: toMultiline(config.platform_build_args?.android ?? []),
    ios_build_args: toMultiline(config.platform_build_args?.ios ?? []),
    macos_build_args: toMultiline(config.platform_build_args?.macos ?? []),
    android_command_override: config.platform_commands?.android ?? '',
    ios_command_override: config.platform_commands?.ios ?? '',
    macos_command_override: config.platform_commands?.macos ?? '',
    env_vars: toMultiline(
      (config.env ?? []).map((entry) => `${entry.key}=${entry.value}`),
    ),
    artifact_patterns: toMultiline(config.artifact_patterns),
    isolation: config.isolation ?? 'host',
  }
}

/** Form fields for imported trigger settings. */
export function triggerFormFields(triggers: TriggerConfig): ImportedFormFields {
  return {
    trigger_events: triggers.events,
    branches: triggers.branches.join(', '),
    tags: (triggers.tags ?? []).join(', '),
    paths: toMultiline(triggers.paths ?? []),
    paths_ignore: toMultiline(triggers.paths_ignore ?? []),
    schedules: (triggers.schedules ?? []).map((schedule) => ({
      cron: schedule.cron,
      branch: schedule.branch ?? '',
    })),
  }
}

/** The fields whose value differs from the form's, so only they are set. */
export function changedFields(
  fields: ImportedFormFields,
  current: PipelineFormValues,
): ImportedFormFields {
  const changed: ImportedFormFields = {}
  forEachField(fields, (name, value) => {
    // Untouched optional fields are `undefined` where imports say ''.
    if (JSON.stringify(current[name] ?? '') !== JSON.stringify(value)) {
      setField(changed, name, value)
    }
  })
  return changed
}

/** What the edit pane shows for the latest import. */
export type ImportOutcome =
  | { kind: 'checking' }
  | { kind: 'error' }
  | { kind: 'invalid'; errors: Array<string> }
  | { kind: 'unsupported'; unsupported: Array<string>; notes: Array<string> }
  | {
      kind: 'ready'
      /** The form fields the file changes; empty when the form matches. */
      fields: ImportedFormFields
      notes: Array<string>
    }

/**
 * Decide what an import means for the form. Only a valid file the form can
 * hold whole changes the form; anything else leaves it as it is.
 */
export function importOutcome({
  isError,
  result,
  current,
  manualOnlyTriggers,
}: {
  isError: boolean
  result: ImportPipelineYamlResponse | undefined
  current: PipelineFormValues
  manualOnlyTriggers: boolean
}): ImportOutcome {
  if (isError) return { kind: 'error' }
  if (!result) return { kind: 'checking' }
  if (!result.valid) {
    return {
      kind: 'invalid',
      errors:
        result.errors.length > 0
          ? result.errors
          : ['The daemon could not read this file.'],
    }
  }
  const config = result.execution_config
  const unsupported = [...result.unsupported]
  if (result.trigger_config && manualOnlyTriggers) {
    unsupported.push(
      'on: this repository only builds manually, so the form has no trigger settings for it',
    )
  }
  if (!result.representable || !config || unsupported.length > 0) {
    if (unsupported.length === 0) {
      unsupported.push('The form cannot hold this file.')
    }
    return { kind: 'unsupported', unsupported, notes: result.notes }
  }
  const fields = executionFormFields(config, current)
  if (result.trigger_config) {
    Object.assign(fields, triggerFormFields(result.trigger_config))
  }
  return {
    kind: 'ready',
    fields: changedFields(fields, current),
    notes: result.notes,
  }
}
