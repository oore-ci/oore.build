import ReactMarkdown from 'react-markdown'
import { Text } from '@/components/layout/text'
import { stackVariants, textVariants } from '@/lib/recipes'
import { cn } from '@/lib/utils'

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
          <ol
            className={cn(
              textVariants({ variant: 'muted' }),
              stackVariants({ gap: 1 }),
              'mt-2 list-decimal pl-5',
            )}
          >
            {listChildren}
          </ol>
        ),
        p: ({ children: paragraphChildren }) => (
          <Text variant="muted" className="mt-2 whitespace-pre-wrap">
            {paragraphChildren}
          </Text>
        ),
        ul: ({ children: listChildren }) => (
          <ul
            className={cn(
              textVariants({ variant: 'muted' }),
              stackVariants({ gap: 1 }),
              'mt-2 list-disc pl-5',
            )}
          >
            {listChildren}
          </ul>
        ),
      }}
    >
      {children}
    </ReactMarkdown>
  )
}
