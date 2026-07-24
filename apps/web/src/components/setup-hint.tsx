import { Info as InformationCircleIcon } from 'lucide-react'
import { Children } from 'react'
import type { ReactNode } from 'react'

import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'

interface SetupHintProps {
  title: string
  children?: ReactNode
  items?: Array<ReactNode>
  code?: string
  className?: string
}

export default function SetupHint({
  title,
  children,
  items,
  code,
  className,
}: SetupHintProps) {
  return (
    <Alert className={className}>
      <InformationCircleIcon />
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
        {code ? (
          <pre className="overflow-x-auto rounded-md bg-background p-3 font-mono text-[11px] leading-relaxed text-foreground ring-1 ring-foreground/10">
            <code>{code}</code>
          </pre>
        ) : null}
      </AlertDescription>
    </Alert>
  )
}
