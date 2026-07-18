type RetentionPolicyLoadState = 'loading' | 'error' | 'ready'

export function resolveRetentionPolicyLoadState(
  isLoading: boolean,
  error: Error | null,
): RetentionPolicyLoadState {
  if (isLoading) return 'loading'
  if (error) return 'error'
  return 'ready'
}
