import * as z from 'zod'

import {
  isHostedUiOrigin,
  isLocalLauncherOrigin,
  isLoopbackUrl,
} from '@/lib/connectivity'

export function addInstanceSchema(frontendOrigin: string) {
  const hostedUi = isHostedUiOrigin(frontendOrigin)
  const localLauncher = isLocalLauncherOrigin(frontendOrigin)

  return z
    .object({
      label: z.string().min(1, 'Label is required'),
      url: z
        .string()
        .transform((v) => v.replace(/\/+$/, ''))
        .pipe(
          z
            .string()
            .refine(
              (v) => v === '' || /^https?:\/\/.+/.test(v),
              'URL must be a valid HTTP/HTTPS URL, or empty for local dev',
            ),
        ),
      icon: z.string(),
    })
    .superRefine((values, ctx) => {
      if (hostedUi) {
        if (!values.url) {
          ctx.addIssue({
            code: z.ZodIssueCode.custom,
            path: ['url'],
            message:
              'Hosted UI requires an explicit HTTPS backend URL (or tunnel URL).',
          })
          return
        }

        if (!values.url.startsWith('http://')) return

        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['url'],
          message:
            'Hosted UI requires an HTTPS backend URL (use a tunnel or reverse proxy).',
        })
        return
      }

      if (!localLauncher || !values.url) return
      if (!isLoopbackUrl(values.url)) return

      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['url'],
        message:
          'When using oore-web locally, leave Backend URL empty to use the built-in proxy.',
      })
    })
}

export type AddInstanceForm = z.infer<ReturnType<typeof addInstanceSchema>>
