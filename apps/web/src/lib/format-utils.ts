export function relativeTime(epochSecs: number): string {
  const diffSecs = Math.floor(Date.now() / 1000) - epochSecs
  if (diffSecs < 5) return 'just now'
  if (diffSecs < 60) return `${diffSecs}s ago`
  const mins = Math.floor(diffSecs / 60)
  if (mins < 60) return `${mins}m ago`
  const hrs = Math.floor(mins / 60)
  if (hrs < 24) return `${hrs}h ago`
  const days = Math.floor(hrs / 24)
  return `${days}d ago`
}

export function formatDuration(seconds: number): string {
  // Round once so 119.6s reads 2m 0s, and clamp runner clock skew at zero.
  const total = Math.max(0, Math.round(seconds))
  if (total < 60) return `${total}s`
  const mins = Math.floor(total / 60)
  const secs = total % 60
  if (mins < 60) return `${mins}m ${secs}s`
  const hrs = Math.floor(mins / 60)
  const remainMins = mins % 60
  return `${hrs}h ${remainMins}m`
}

export function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  if (bytes < 1024 * 1024 * 1024)
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`
}

export function formatAbsoluteTime(epochSecs: number): string {
  return new Date(epochSecs * 1000).toLocaleString()
}

/**
 * Turns a machine enum such as `timed_out` or `pull_request` into a
 * sentence-case label. Known enums get an explicit label map instead; this is
 * the fallback for values the UI has no dedicated wording for.
 */
export function formatEnumLabel(value: string): string {
  const words = value.trim().replace(/[_-]+/g, ' ').replace(/\s+/g, ' ')
  if (!words) return ''
  return words.charAt(0).toUpperCase() + words.slice(1).toLowerCase()
}
