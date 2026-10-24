import * as z from 'zod'

export const TRIGGER_EVENTS = ['push', 'pull_request', 'tag_push'] as const

/** Starts a build can wait on: the webhook events plus manual and scheduled. */
export const APPROVAL_EVENTS = [
  'push',
  'pull_request',
  'tag_push',
  'manual',
  'schedule',
] as const

export const BUILD_ISOLATIONS = ['host', 'vm'] as const

export const DEFAULT_TIMEOUT_MINUTES = 60
export const MAX_TIMEOUT_MINUTES = 1440
export const MAX_SCHEDULES = 10
export const MAX_RUNNER_LABELS = 32

const RUNNER_LABEL = /^[a-z0-9._-]{1,64}$/
const CRON_MACRO = /^@(hourly|daily|weekly|monthly|yearly|annually)$/

/** Five fields or a macro; the server checks the fields themselves. */
export function isCronExpression(value: string): boolean {
  const trimmed = value.trim()
  return CRON_MACRO.test(trimmed) || trimmed.split(/\s+/).length === 5
}

export function isRunnerLabel(value: string): boolean {
  return RUNNER_LABEL.test(value.trim().toLowerCase())
}

export const scheduleSchema = z.object({
  cron: z
    .string()
    .trim()
    .min(1, 'Cron expression is required')
    .refine(
      isCronExpression,
      'Use five fields (minute hour day month weekday) or @hourly, @daily, @weekly, @monthly',
    ),
  branch: z.string().optional(),
})

export const SIZE_BUDGET_MODES = ['warn', 'fail'] as const
export const MAX_SIZE_BUDGET_RULES = 20

const SIZE_BUDGET_KIND = /^[a-z0-9]{1,16}$/

/** 1 TB: far above any app, and well inside a safe integer of bytes. */
export const MAX_SIZE_BUDGET_MB = 1_048_576
export const MAX_SIZE_BUDGET_PERCENT = 100_000
const MEBIBYTE = 1024 * 1024

/** Empty, or a number that passes `valid`. */
function optionalAmount(valid: (amount: number) => boolean, message: string) {
  return z
    .string()
    .optional()
    .refine((value) => {
      const trimmed = value?.trim()
      if (!trimmed) return true
      const amount = Number(trimmed)
      return Number.isFinite(amount) && valid(amount)
    }, message)
}

/** One size budget rule. Sizes are in MB (1,048,576 bytes), as Oore shows them. */
export const sizeBudgetRuleSchema = z
  .object({
    kind: z
      .string()
      .trim()
      .regex(SIZE_BUDGET_KIND, 'Use a lowercase file extension, such as aab'),
    // The server stores bytes, so a size must round to at least one byte.
    max_mb: optionalAmount(
      (mb) => Math.round(mb * MEBIBYTE) >= 1 && mb <= MAX_SIZE_BUDGET_MB,
      `Must be more than 0 and at most ${MAX_SIZE_BUDGET_MB.toLocaleString('en-US')}`,
    ),
    max_increase_mb: optionalAmount(
      (mb) => mb >= 0 && mb <= MAX_SIZE_BUDGET_MB,
      `Must be 0 to ${MAX_SIZE_BUDGET_MB.toLocaleString('en-US')}`,
    ),
    max_increase_percent: optionalAmount(
      (percent) => percent >= 0 && percent <= MAX_SIZE_BUDGET_PERCENT,
      `Must be 0 to ${MAX_SIZE_BUDGET_PERCENT.toLocaleString('en-US')}`,
    ),
  })
  .refine(
    (rule) =>
      [rule.max_mb, rule.max_increase_mb, rule.max_increase_percent].some(
        (value) => value?.trim(),
      ),
    { message: 'Set at least one limit', path: ['max_mb'] },
  )

export const pipelineFormSchema = z
  .object({
    name: z.string().min(1, 'Name is required'),
    config_mode: z.enum(['auto', 'explicit']),
    config_path: z.string().optional(),
    /** A `.oore.yaml` version 2 workflow; empty builds the file's default. */
    workflow: z.string().optional(),
    platform_android: z.boolean(),
    platform_ios: z.boolean(),
    platform_macos: z.boolean(),
    android_signing_release_enabled: z.boolean(),
    android_signing_release_store_password: z.string().optional(),
    android_signing_release_key_alias: z.string().optional(),
    android_signing_release_key_password: z.string().optional(),
    android_signing_debug_enabled: z.boolean(),
    android_signing_debug_store_password: z.string().optional(),
    android_signing_debug_key_alias: z.string().optional(),
    android_signing_debug_key_password: z.string().optional(),
    ios_signing_enabled: z.boolean(),
    ios_signing_mode: z.enum(['manual', 'api', 'hybrid']),
    ios_signing_export_method: z.enum(['ad_hoc', 'app_store']),
    ios_signing_team_id: z.string().optional(),
    ios_signing_bundle_ids: z.string().optional(),
    ios_signing_p12_password: z.string().optional(),
    ios_signing_api_key_id: z.string().optional(),
    ios_signing_api_issuer_id: z.string().optional(),
    /** A shared App Store Connect key id, `own`, or empty until chosen. */
    ios_signing_api_key_source: z.string().optional(),
    flutter_version: z
      .string()
      .optional()
      .refine((v) => !v || v.trim().length <= 64, 'Max 64 characters'),
    build_number_offset: z
      .string()
      .optional()
      .refine(
        (v) =>
          !v?.trim() ||
          (/^\d+$/.test(v.trim()) && Number(v.trim()) < 2_100_000_000),
        'Must be a whole number below 2,100,000,000',
      ),
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
    isolation: z.enum(BUILD_ISOLATIONS),
    runs_on: z
      .string()
      .optional()
      .refine((v) => {
        const labels = (v ?? '')
          .split(',')
          .map((l) => l.trim())
          .filter(Boolean)
        return labels.length <= MAX_RUNNER_LABELS && labels.every(isRunnerLabel)
      }, 'Labels use letters, digits, ".", "_" and "-", up to 64 characters each'),
    timeout_minutes: z
      .string()
      .refine(
        (v) =>
          /^\d+$/.test(v.trim()) &&
          Number(v) >= 1 &&
          Number(v) <= MAX_TIMEOUT_MINUTES,
        `Must be a whole number of minutes between 1 and ${MAX_TIMEOUT_MINUTES}`,
      ),
    trigger_events: z.array(z.string()),
    tags: z.string().optional(),
    paths: z.string().optional(),
    paths_ignore: z.string().optional(),
    schedules: z
      .array(scheduleSchema)
      .max(MAX_SCHEDULES, `At most ${MAX_SCHEDULES} schedules`),
    approval_required: z.boolean(),
    approval_events: z.array(z.string()),
    size_budget_mode: z.enum(SIZE_BUDGET_MODES),
    size_budget_rules: z
      .array(sizeBudgetRuleSchema)
      .max(MAX_SIZE_BUDGET_RULES, `At most ${MAX_SIZE_BUDGET_RULES} rules`),
    cancel_previous: z.boolean(),
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
    const kinds = new Set<string>()
    data.size_budget_rules.forEach((rule, index) => {
      const kind = rule.kind.trim()
      if (kinds.has(kind)) {
        ctx.addIssue({
          code: 'custom',
          message: 'Another rule already covers this kind',
          path: ['size_budget_rules', index, 'kind'],
        })
      }
      kinds.add(kind)
    })
    if (data.config_mode === 'explicit' && !data.config_path?.trim()) {
      ctx.addIssue({
        code: 'custom',
        message: 'Config path is required when explicit mode is selected',
        path: ['config_path'],
      })
    }
    if (data.ios_signing_enabled && !data.platform_ios) {
      ctx.addIssue({
        code: 'custom',
        message: 'iOS signing requires iOS platform to be enabled',
        path: ['ios_signing_enabled'],
      })
    }
    if (data.ios_signing_enabled) {
      if (!data.ios_signing_team_id?.trim()) {
        ctx.addIssue({
          code: 'custom',
          message: 'Team ID is required when iOS signing is enabled',
          path: ['ios_signing_team_id'],
        })
      }
      if (!data.ios_signing_bundle_ids?.trim()) {
        ctx.addIssue({
          code: 'custom',
          message: 'At least one bundle ID is required',
          path: ['ios_signing_bundle_ids'],
        })
      }
      if (
        (data.ios_signing_mode === 'api' ||
          data.ios_signing_mode === 'hybrid') &&
        usesOwnApiKey(data.ios_signing_api_key_source)
      ) {
        if (!data.ios_signing_api_key_id?.trim()) {
          ctx.addIssue({
            code: 'custom',
            message: 'API Key ID is required for API/Hybrid mode',
            path: ['ios_signing_api_key_id'],
          })
        }
        if (!data.ios_signing_api_issuer_id?.trim()) {
          ctx.addIssue({
            code: 'custom',
            message: 'Issuer ID is required for API/Hybrid mode',
            path: ['ios_signing_api_issuer_id'],
          })
        }
      }
    }
  })

export type PipelineFormValues = z.infer<typeof pipelineFormSchema>

/** Whether iOS signing uses the pipeline's own key rather than a shared one. */
export function usesOwnApiKey(source: string | undefined) {
  return !source || source === 'own'
}
