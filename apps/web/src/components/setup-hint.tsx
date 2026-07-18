import { InformationCircleIcon } from '@hugeicons/core-free-icons'
import { HugeiconsIcon } from '@hugeicons/react'
import { Children } from 'react'
import type { ReactNode } from 'react'

import { cn } from '@/lib/utils'

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
    <div
      className={cn(
        'border border-border/60 bg-muted/20 p-3 text-xs text-muted-foreground [&_code]:break-all',
        className,
      )}
    >
      <div className="flex items-center gap-2 text-foreground">
        <HugeiconsIcon
          icon={InformationCircleIcon}
          size={14}
          className="shrink-0 text-muted-foreground"
        />
        <p className="font-medium">{title}</p>
      </div>
      {children ? <div className="mt-2 leading-relaxed">{children}</div> : null}
      {items && items.length > 0 ? (
        <ul className="mt-2 list-disc space-y-1 pl-5 leading-relaxed">
          {Children.map(items, (item) => (
            <li>{item}</li>
          ))}
        </ul>
      ) : null}
      {code ? (
        <pre className="mt-3 overflow-x-auto border border-border/60 bg-background p-3 font-mono text-[11px] leading-relaxed text-foreground">
          <code>{code}</code>
        </pre>
      ) : null}
    </div>
  )
}
