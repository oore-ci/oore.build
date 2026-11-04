import * as z from 'zod'

/** The Run build drawer's fields. */
export const triggerBuildSchema = z
  .object({
    project_id: z.string().optional(),
    pipeline_id: z.string().optional(),
    platforms: z.array(z.enum(['android', 'ios', 'macos'])),
    branch: z.string().optional(),
    commit_sha: z.string().optional(),
    workflow: z.string().optional(),
    changelog: z
      .string()
      .max(4000, 'Keep the changelog under 4,000 characters')
      .optional(),
  })
  .superRefine((data, ctx) => {
    const branch = data.branch?.trim()
    const commit = data.commit_sha?.trim()
    if (!branch && !commit) {
      ctx.addIssue({
        code: 'custom',
        message: 'Provide a branch or a commit SHA',
        path: ['branch'],
      })
    }
  })

export type TriggerBuildForm = z.infer<typeof triggerBuildSchema>
