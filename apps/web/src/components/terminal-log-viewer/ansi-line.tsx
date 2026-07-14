import { parseAnsi } from '@/lib/ansi-to-html'

export function AnsiLine({ content }: { content: string }) {
  const spans = parseAnsi(content)
  if (spans.length === 1 && !spans[0].fg && !spans[0].bold) return content

  return spans.map((span, index) => {
    const style: React.CSSProperties = {}
    if (span.fg) style.color = span.fg
    if (span.bg) style.backgroundColor = span.bg
    if (span.bold) style.fontWeight = 700
    if (span.dim) style.opacity = 0.6
    if (span.italic) style.fontStyle = 'italic'
    if (span.underline) style.textDecoration = 'underline'

    return Object.keys(style).length > 0 ? (
      <span key={index} style={style}>
        {span.text}
      </span>
    ) : (
      <span key={index}>{span.text}</span>
    )
  })
}
