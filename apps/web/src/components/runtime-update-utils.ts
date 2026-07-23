export function formatReleaseNotes(notes: string): string {
  return notes
    .replace(/^#{1,6}\s+.*(?:\r?\n)+/, '')
    .replace(/^\*\*Full Changelog\*\*:.*$/gim, '')
    .replace(/\[([^\]]+)]\([^)]+\)/g, '$1')
    .replace(/\*\*([^*]+)\*\*/g, '$1')
    .trim()
}

export function installerCommand(channel: string): string {
  if (channel === 'alpha' || channel === 'beta') {
    return `curl -fsSL https://${channel}.oore.pages.dev/install | OORE_CHANNEL=${channel} bash`
  }
  return 'curl -fsSL https://oore.build/install | bash'
}
