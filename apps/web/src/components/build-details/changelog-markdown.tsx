import ReactMarkdown from 'react-markdown'

export default function ChangelogMarkdown({ children }: { children: string }) {
  return (
    <ReactMarkdown
      skipHtml
      components={{
        a: ({ children: linkChildren, ...props }) => (
          <a
            {...props}
            target="_blank"
            rel="noreferrer"
            className="underline underline-offset-4"
          >
            {linkChildren}
          </a>
        ),
        ol: ({ children: listChildren }) => (
          <ol className="mt-2 flex list-decimal flex-col gap-1 pl-5 text-sm text-muted-foreground">
            {listChildren}
          </ol>
        ),
        p: ({ children: paragraphChildren }) => (
          <p className="mt-2 whitespace-pre-wrap text-sm text-muted-foreground">
            {paragraphChildren}
          </p>
        ),
        ul: ({ children: listChildren }) => (
          <ul className="mt-2 flex list-disc flex-col gap-1 pl-5 text-sm text-muted-foreground">
            {listChildren}
          </ul>
        ),
      }}
    >
      {children}
    </ReactMarkdown>
  )
}
