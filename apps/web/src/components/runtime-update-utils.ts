export function formatReleaseNotes(notes: string): string {
  return notes
    .replace(/^#{1,6}\s+.*(?:\r?\n)+/, '')
    .replace(/^\*\*Full Changelog\*\*:.*$/gim, '')
    .replace(/\[([^\]]+)]\([^)]+\)/g, '$1')
    .replace(/\*\*([^*]+)\*\*/g, '$1')
    .trim()
}
