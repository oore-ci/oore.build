/**
 * Lightweight ANSI SGR -> React-friendly spans converter.
 * Handles colors (30-37, 90-97, 40-47, 100-107), bold (1), dim (2),
 * italic (3), underline (4), and reset (0).
 */

export interface AnsiSpan {
  text: string
  bold?: boolean
  dim?: boolean
  italic?: boolean
  underline?: boolean
  fg?: string
  bg?: string
}

const ANSI_COLORS: Record<number, string> = {
  30: '#4e4e4e', // black
  31: '#ff6b6b', // red
  32: '#69db7c', // green
  33: '#ffd43b', // yellow
  34: '#74c0fc', // blue
  35: '#da77f2', // magenta
  36: '#66d9e8', // cyan
  37: '#dee2e6', // white
  90: '#868e96', // bright black
  91: '#ff8787', // bright red
  92: '#8ce99a', // bright green
  93: '#ffe066', // bright yellow
  94: '#a5d8ff', // bright blue
  95: '#e599f7', // bright magenta
  96: '#99e9f2', // bright cyan
  97: '#f8f9fa', // bright white
}

const ANSI_BG_COLORS: Record<number, string> = {
  40: '#4e4e4e',
  41: '#c92a2a',
  42: '#2b8a3e',
  43: '#e67700',
  44: '#1864ab',
  45: '#862e9c',
  46: '#0b7285',
  47: '#dee2e6',
  100: '#868e96',
  101: '#ff6b6b',
  102: '#69db7c',
  103: '#ffd43b',
  104: '#74c0fc',
  105: '#da77f2',
  106: '#66d9e8',
  107: '#f8f9fa',
}

// Match ESC[ ... m sequences
const ANSI_RE = /\x1b\[([0-9;]*)m/g

export function parseAnsi(input: string): AnsiSpan[] {
  const spans: AnsiSpan[] = []
  let bold = false
  let dim = false
  let italic = false
  let underline = false
  let fg: string | undefined
  let bg: string | undefined
  let lastIndex = 0

  for (const match of input.matchAll(ANSI_RE)) {
    // Push text before this escape
    if (match.index > lastIndex) {
      const text = input.slice(lastIndex, match.index)
      if (text) {
        spans.push({ text, bold, dim, italic, underline, fg, bg })
      }
    }
    lastIndex = match.index + match[0].length

    // Parse SGR codes
    const codes = match[1]
      ? match[1].split(';').map(Number)
      : [0]

    for (const code of codes) {
      if (code === 0) {
        bold = false
        dim = false
        italic = false
        underline = false
        fg = undefined
        bg = undefined
      } else if (code === 1) {
        bold = true
      } else if (code === 2) {
        dim = true
      } else if (code === 3) {
        italic = true
      } else if (code === 4) {
        underline = true
      } else if (code === 22) {
        bold = false
        dim = false
      } else if (code === 23) {
        italic = false
      } else if (code === 24) {
        underline = false
      } else if (code === 39) {
        fg = undefined
      } else if (code === 49) {
        bg = undefined
      } else if (ANSI_COLORS[code]) {
        fg = ANSI_COLORS[code]
      } else if (ANSI_BG_COLORS[code]) {
        bg = ANSI_BG_COLORS[code]
      }
    }
  }

  // Push remaining text
  if (lastIndex < input.length) {
    const text = input.slice(lastIndex)
    if (text) {
      spans.push({ text, bold, dim, italic, underline, fg, bg })
    }
  }

  // If no ANSI was found, return a single span
  if (spans.length === 0 && input) {
    spans.push({ text: input })
  }

  return spans
}

export function hasAnsiCodes(input: string): boolean {
  return /\x1b\[/.test(input)
}
