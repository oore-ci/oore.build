import { Badge } from '@/components/ui/badge'
import { Skeleton } from '@/components/ui/skeleton'

export function ApiTokenStats({
  active,
  isLoading,
  revoked,
  total,
}: {
  active: number
  isLoading: boolean
  revoked: number
  total: number
}) {
  const metrics = [
    ['Total tokens', total, 'Active, expired, and revoked'],
    ['Active tokens', active, 'Currently valid for API access'],
    ['Revoked tokens', revoked, 'No longer valid'],
  ] as const
  return (
    <section
      aria-label="API token summary"
      className="grid border md:grid-cols-3 md:divide-x"
    >
      {metrics.map(([label, value, description]) => (
        <div
          key={label}
          className="border-t p-4 first:border-t-0 md:border-t-0"
        >
          {isLoading ? (
            <div className="space-y-2">
              <Skeleton className="h-3 w-20" />
              <Skeleton className="h-6 w-10" />
              <Skeleton className="h-3 w-32" />
            </div>
          ) : (
            <>
              <div className="flex items-center justify-between">
                <p className="text-xs font-medium text-muted-foreground">
                  {label}
                </p>
                {label === 'Active tokens' && value > 0 ? (
                  <Badge variant="secondary">{value}</Badge>
                ) : null}
              </div>
              <p className="mt-2 text-xl font-semibold tracking-tight">
                {value}
              </p>
              <p className="mt-1 text-xs text-muted-foreground">
                {description}
              </p>
            </>
          )}
        </div>
      ))}
    </section>
  )
}
