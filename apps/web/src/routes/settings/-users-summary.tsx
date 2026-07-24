import { Skeleton } from '@/components/ui/skeleton'
import { Card, CardContent } from '@/components/ui/card'

export function UsersSummary({
  counts,
  isLoading,
}: {
  counts: { active: number; invited: number; total: number }
  isLoading: boolean
}) {
  const metrics = [
    ['Total', counts.total, 'All instance users'],
    ['Active', counts.active, 'Can currently sign in'],
    ['Invited', counts.invited, 'Awaiting activation'],
  ] as const
  return (
    <Card size="sm" aria-label="User summary">
      <CardContent className="grid grid-cols-3 divide-x px-0">
        {isLoading
          ? Array.from({ length: 3 }, (_, index) => (
              <div key={index} className="p-4">
                <div className="flex flex-col gap-2">
                  <Skeleton className="h-3 w-14" />
                  <Skeleton className="h-7 w-10" />
                  <Skeleton className="hidden h-3 w-24 md:block" />
                </div>
              </div>
            ))
          : metrics.map(([label, value, description]) => (
              <div key={label} className="p-4">
                <p className="text-xs font-medium text-muted-foreground">
                  {label}
                </p>
                <p className="mt-2 text-xl font-semibold tracking-tight">
                  {value}
                </p>
                <p className="mt-1 hidden text-xs text-muted-foreground md:block">
                  {description}
                </p>
              </div>
            ))}
      </CardContent>
    </Card>
  )
}
