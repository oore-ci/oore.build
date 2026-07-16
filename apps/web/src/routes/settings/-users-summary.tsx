import { Card, CardContent } from '@/components/ui/card'
import { Skeleton } from '@/components/ui/skeleton'

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
    <section
      aria-label="User summary"
      className="grid grid-cols-3 gap-2 md:gap-4"
    >
      {isLoading
        ? Array.from({ length: 3 }, (_, index) => (
            <Card key={index}>
              <CardContent>
                <div className="flex flex-col gap-2">
                  <Skeleton className="h-3 w-14" />
                  <Skeleton className="h-7 w-10" />
                  <Skeleton className="hidden h-3 w-24 md:block" />
                </div>
              </CardContent>
            </Card>
          ))
        : metrics.map(([label, value, description]) => (
            <Card key={label}>
              <CardContent>
                <p className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                  {label}
                </p>
                <p className="mt-3 text-2xl font-bold tracking-tight">
                  {value}
                </p>
                <p className="mt-1 hidden text-xs text-muted-foreground md:block">
                  {description}
                </p>
              </CardContent>
            </Card>
          ))}
    </section>
  )
}
