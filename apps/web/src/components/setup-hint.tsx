import { HugeiconsIcon } from '@hugeicons/react'
import { InformationCircleIcon } from '@hugeicons/core-free-icons'
import { Children, useState } from 'react'
import type { ReactNode } from 'react'

import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { monoVariants } from '@/lib/recipes'
import { cn } from '@/lib/utils'

interface SetupHintProps {
  title: string
  /**
   * One line shown in place of the note. The full note opens from it, so
   * long guidance does not sit louder than the fields around it.
   */
  summary?: ReactNode
  children?: ReactNode
  items?: Array<ReactNode>
  code?: string
  className?: string
}

function CodeBlock({ code }: { code: string }) {
  return (
    <pre
      className={cn(
        monoVariants({ size: '2xs' }),
        'overflow-x-auto rounded-md bg-background p-3 leading-relaxed text-foreground ring-1 ring-foreground/10',
      )}
    >
      <code>{code}</code>
    </pre>
  )
}

export default function SetupHint({
  title,
  summary,
  children,
  items,
  code,
  className,
}: SetupHintProps) {
  const [open, setOpen] = useState(false)
  if (summary && !open) {
    return (
      <p
        className={cn(
          'flex items-start gap-2 text-sm text-muted-foreground',
          className,
        )}
      >
        <HugeiconsIcon
          icon={InformationCircleIcon}
          className="mt-0.5 size-4 shrink-0"
          aria-hidden
        />
        <span>
          {summary}{' '}
          <button
            type="button"
            className="font-medium text-foreground underline-offset-4 hover:underline"
            onClick={() => setOpen(true)}
          >
            More about {title.charAt(0).toLowerCase() + title.slice(1)}
          </button>
        </span>
      </p>
    )
  }
  // Static guidance: a note, not a live alert that screen readers announce
  // when the page loads.
  return (
    <Alert role="note" className={className}>
      <HugeiconsIcon icon={InformationCircleIcon} />
      <AlertTitle>{title}</AlertTitle>
      <AlertDescription className="[&_code]:break-all">
        {children}
        {items && items.length > 0 ? (
          <ul className="list-disc space-y-1 pl-5">
            {Children.map(items, (item) => (
              <li>{item}</li>
            ))}
          </ul>
        ) : null}
        {code ? <CodeBlock code={code} /> : null}
      </AlertDescription>
    </Alert>
  )
}
