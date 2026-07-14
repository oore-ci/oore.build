import z from 'zod'

import { normalizeGitLabHostUrl } from '@/lib/gitlab-url'

export const gitLabSetupSchema = z
  .object({
    host_url: z
      .string()
      .trim()
      .min(1, 'Host URL is required')
      .refine(
        (value) => normalizeGitLabHostUrl(value) !== null,
        'Use an HTTP(S) host URL only, without a path, query, or credentials.',
      ),
    auth_mode: z.enum(['personal_token', 'oauth_app']),
    webhook_secret: z.string().trim().min(1, 'Webhook secret is required'),
    access_token: z.string().optional(),
    client_id: z.string().optional(),
    client_secret: z.string().optional(),
  })
  .superRefine((value, ctx) => {
    if (value.auth_mode === 'personal_token' && !value.access_token?.trim()) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        message: 'Access token is required',
        path: ['access_token'],
      })
    }

    if (value.auth_mode === 'oauth_app' && !value.client_id?.trim()) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        message: 'Client ID is required',
        path: ['client_id'],
      })
    }

    if (value.auth_mode === 'oauth_app' && !value.client_secret?.trim()) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        message: 'Client secret is required',
        path: ['client_secret'],
      })
    }
  })

export type GitLabSetupForm = z.infer<typeof gitLabSetupSchema>
export type GitLabHostKind = 'gitlab_com' | 'self_managed'

export function generateWebhookSecret(): string {
  const bytes = crypto.getRandomValues(new Uint8Array(24))
  return `oore_${Array.from(bytes, (byte) => byte.toString(16).padStart(2, '0')).join('')}`
}
