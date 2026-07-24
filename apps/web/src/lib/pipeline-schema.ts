import * as z from 'zod'

export const TRIGGER_EVENTS = ['push', 'pull_request', 'tag_push'] as const

export const pipelineFormSchema = z
  .object({
    name: z.string().min(1, 'Name is required'),
    config_mode: z.enum(['auto', 'explicit']),
    config_path: z.string().optional(),
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
    ios_signing_team_id: z.string().optional(),
    ios_signing_bundle_ids: z.string().optional(),
    ios_signing_p12_password: z.string().optional(),
    ios_signing_api_key_id: z.string().optional(),
    ios_signing_api_issuer_id: z.string().optional(),
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
    trigger_events: z.array(z.string()),
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
    if (data.config_mode === 'explicit' && !data.config_path?.trim()) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        message: 'Config path is required when explicit mode is selected',
        path: ['config_path'],
      })
    }
    if (data.ios_signing_enabled && !data.platform_ios) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        message: 'iOS signing requires iOS platform to be enabled',
        path: ['ios_signing_enabled'],
      })
    }
    if (data.ios_signing_enabled) {
      if (!data.ios_signing_team_id?.trim()) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          message: 'Team ID is required when iOS signing is enabled',
          path: ['ios_signing_team_id'],
        })
      }
      if (!data.ios_signing_bundle_ids?.trim()) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          message: 'At least one bundle ID is required',
          path: ['ios_signing_bundle_ids'],
        })
      }
      if (
        data.ios_signing_mode === 'api' ||
        data.ios_signing_mode === 'hybrid'
      ) {
        if (!data.ios_signing_api_key_id?.trim()) {
          ctx.addIssue({
            code: z.ZodIssueCode.custom,
            message: 'API Key ID is required for API/Hybrid mode',
            path: ['ios_signing_api_key_id'],
          })
        }
        if (!data.ios_signing_api_issuer_id?.trim()) {
          ctx.addIssue({
            code: z.ZodIssueCode.custom,
            message: 'Issuer ID is required for API/Hybrid mode',
            path: ['ios_signing_api_issuer_id'],
          })
        }
      }
    }
  })

export type PipelineFormValues = z.infer<typeof pipelineFormSchema>
