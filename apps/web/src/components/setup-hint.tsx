import { HugeiconsIcon } from '@hugeicons/react'
import { InformationCircleIcon } from '@hugeicons/core-free-icons'
import { Children } from 'react'
import type { ReactNode } from 'react'

import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { monoVariants } from '@/lib/recipes'
import { cn } from '@/lib/utils'

interface SetupHintProps {
  title: string
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
  children,
  items,
  code,
  className,
}: SetupHintProps) {
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
