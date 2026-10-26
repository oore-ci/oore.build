import { isOoreApiError } from '@oore/client/client'

export function getApiErrorMessage(
  cause: unknown,
  codeMap: Record<string, string>,
): string {
  if (isOoreApiError(cause) && cause.code) {
    return codeMap[cause.code] ?? cause.message
  }
  if (cause instanceof Error) return cause.message
  return 'An unexpected error occurred. Please try again.'
}
