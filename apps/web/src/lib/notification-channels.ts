import type {
  CreateNotificationChannelRequest as ClientCreateNotificationChannelRequest,
  NotificationChannel as ClientNotificationChannel,
  NotificationChannelType,
  UpdateNotificationChannelRequest as ClientUpdateNotificationChannelRequest,
} from '@oore/client/models'

// The daemon contract has per-channel Slack member mappings, but the
// published @oore/client models predate them. Drop these once the client is
// regenerated.
type Flatten<T> = { [K in keyof T]: T[K] }

export type { NotificationChannelType }

export interface SlackMention {
  /** Oore sign-in email or Git username, matched without regard to case. */
  identity: string
  /** Slack member ID, such as U0123ABCD. */
  member_id: string
}

export type NotificationChannel = Flatten<
  ClientNotificationChannel & { slack_mentions: Array<SlackMention> }
>

export type CreateNotificationChannelRequest = Flatten<
  ClientCreateNotificationChannelRequest & {
    slack_mentions?: Array<SlackMention>
  }
>

export type UpdateNotificationChannelRequest = Flatten<
  ClientUpdateNotificationChannelRequest & {
    slack_mentions?: Array<SlackMention>
  }
>

export function toClientCreateRequest(
  request: CreateNotificationChannelRequest,
): ClientCreateNotificationChannelRequest {
  // SAFETY: the daemon accepts every field here; only the client's generated
  // types lag behind, and operations do not validate request bodies.
  return request as ClientCreateNotificationChannelRequest
}

export function toClientUpdateRequest(
  request: UpdateNotificationChannelRequest,
): ClientUpdateNotificationChannelRequest {
  // SAFETY: as above.
  return request as ClientUpdateNotificationChannelRequest
}

export function fromClientChannel(
  channel: ClientNotificationChannel,
): NotificationChannel {
  // SAFETY: the daemon returns every field here; only the client's generated
  // types lag behind.
  return channel as NotificationChannel
}

const SLACK_MEMBER_ID = /^[UW][A-Z0-9]{2,23}$/
// Matches the daemon's limit.
const MAX_SLACK_MENTIONS = 500

export type SlackMentionsParseResult =
  | { ok: true; mentions: Array<SlackMention> }
  | { ok: false; error: string }

/**
 * Reads one mapping per line: a sign-in email or Git username, then a Slack
 * member ID, separated by whitespace. Only whitespace separates them, since
 * `=` and `,` are legal in an email. Blank lines are skipped.
 */
export function parseSlackMentions(text: string): SlackMentionsParseResult {
  const mentions: Array<SlackMention> = []
  const lines = text.split('\n')
  for (const [index, raw] of lines.entries()) {
    const line = raw.trim()
    if (!line) continue
    const parts = line.split(/\s+/)
    const [identity, memberId] = parts
    if (parts.length !== 2 || !identity || !memberId) {
      return {
        ok: false,
        error: `Line ${index + 1}: use "email-or-git-username U0123ABCD"`,
      }
    }
    if (!SLACK_MEMBER_ID.test(memberId)) {
      return {
        ok: false,
        error: `Line ${index + 1}: ${memberId} is not a Slack member ID (for example U0123ABCD)`,
      }
    }
    const key = identity.toLowerCase()
    if (mentions.some((mention) => mention.identity === key)) {
      return {
        ok: false,
        error: `Line ${index + 1}: ${identity} is listed twice`,
      }
    }
    mentions.push({ identity: key, member_id: memberId })
  }
  if (mentions.length > MAX_SLACK_MENTIONS) {
    return {
      ok: false,
      error: `List at most ${MAX_SLACK_MENTIONS} people (${mentions.length} listed)`,
    }
  }
  return { ok: true, mentions }
}

export function formatSlackMentions(
  mentions: Array<SlackMention> | undefined,
): string {
  return (mentions ?? [])
    .map((mention) => `${mention.identity} ${mention.member_id}`)
    .join('\n')
}
