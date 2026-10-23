export function formatReleaseNotes(notes: string): string {
  return (
    notes
      .replace(/^#{1,6}\s+.*(?:\r?\n)+/, '')
      // Each entry already names its release, so the range line is noise.
      .replace(/^Changes since \S+:\s*(?:\r?\n)+/i, '')
      .replace(/^\*\*Full Changelog\*\*:.*$/gim, '')
      .replace(/\[([^\]]+)]\([^)]+\)/g, '$1')
      .replace(/\*\*([^*]+)\*\*/g, '$1')
      .trim()
  )
}
