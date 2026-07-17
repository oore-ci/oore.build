export const DIRECT_RUNNER_TRUST_NOTICE_VERSION = 'direct-runner-protocol-4'

export function directRunnerTrustNoticeKey(
  instanceId: string,
  userId: string,
): string {
  return `${DIRECT_RUNNER_TRUST_NOTICE_VERSION}:${instanceId}:${userId}`
}

export function shouldShowDirectRunnerTrustNotice(
  role: string | undefined,
  noticeKey: string | null,
  acknowledgements: Record<string, true>,
): boolean {
  return (
    (role === 'owner' || role === 'admin') &&
    noticeKey !== null &&
    !acknowledgements[noticeKey]
  )
}
