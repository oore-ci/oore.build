import { cn } from '@/lib/utils'
import { Row } from '@/components/layout/row'
import { Stack } from '@/components/layout/stack'
import { Text } from '@/components/layout/text'
import { textVariants } from '@/lib/recipes'

interface PageHeaderProps {
  title: string
  description?: string
  actions?: React.ReactNode
  meta?: React.ReactNode
  divided?: boolean
}

export default function PageHeader({
  title,
  description,
  actions,
  meta,
  divided = false,
}: PageHeaderProps) {
  return (
    <header className={cn('space-y-3', divided && 'border-b pb-5')}>
      <Stack gap={4} className="sm:flex-row sm:items-start sm:justify-between">
        <div className="min-w-0 space-y-1">
          <h1 className="text-2xl font-semibold tracking-tight">{title}</h1>
          {description ? (
            <Text variant="muted" className="max-w-[65ch]">
              {description}
            </Text>
          ) : null}
        </div>
        {actions ? (
          <Row gap={2} className="w-full flex-wrap sm:w-auto sm:justify-end">
            {actions}
          </Row>
        ) : null}
      </Stack>

      {meta ? (
        <Row
          gap={2}
          className={cn(textVariants({ variant: 'caption' }), 'flex-wrap pt-2')}
        >
          {meta}
        </Row>
      ) : null}
    </header>
  )
}
