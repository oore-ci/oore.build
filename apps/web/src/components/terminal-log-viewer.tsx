import { For } from 'solid-js'

export default function TerminalLogViewer(props: {
  lines: Array<{ sequence: number; content: string; stream: string }>
  class?: string
}) {
  return (
    <div class={`overflow-hidden border bg-black ${props.class ?? ''}`}>
      <div class="flex items-center justify-between border-b border-zinc-800 px-3 py-2 text-xs text-zinc-300">
        <span>Logs</span>
        <span>{props.lines.length} lines</span>
      </div>
      <pre class="max-h-[72vh] overflow-auto p-3 font-mono text-xs text-green-300">
        <For each={props.lines}>
          {(line) => (
            <div>
              <span class="text-zinc-500">
                {line.sequence.toString().padStart(4, '0')}
              </span>{' '}
              <span>{line.content}</span>
            </div>
          )}
        </For>
      </pre>
    </div>
  )
}
