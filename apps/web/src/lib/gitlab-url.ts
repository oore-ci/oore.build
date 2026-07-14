export function normalizeGitLabHostUrl(value: string): string | null {
  try {
    const url = new URL(value.trim())
    if (
      !['http:', 'https:'].includes(url.protocol) ||
      !url.hostname ||
      url.username ||
      url.password ||
      url.search ||
      url.hash ||
      !/^\/+$/u.test(url.pathname)
    ) {
      return null
    }
    return url.origin
  } catch {
    return null
  }
}

export function gitLabPublicEndpoints(
  configuredPublicUrl: string | undefined,
  currentOrigin: string,
) {
  const origin = (configuredPublicUrl?.trim() || currentOrigin).replace(
    /\/+$/,
    '',
  )
  return {
    callbackUrl: `${origin}/v1/integrations/gitlab/callback`,
    webhookUrl: `${origin}/v1/webhooks/gitlab`,
  }
}
