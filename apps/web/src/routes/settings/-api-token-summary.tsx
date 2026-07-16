import { Badge } from '@/components/ui/badge'
import { Card, CardContent } from '@/components/ui/card'

export function ApiTokenStats({
  active,
  revoked,
  total,
}: {
  active: number
  revoked: number
  total: number
}) {
  const metrics = [
    ['Total tokens', total, 'Active, expired, and revoked'],
    ['Active tokens', active, 'Currently valid for API access'],
    ['Revoked tokens', revoked, 'No longer valid'],
  ] as const
  return (
    <section className="grid gap-4 md:grid-cols-3">
      {metrics.map(([label, value, description]) => (
        <Card key={label}>
          <CardContent>
            <div className="flex items-center justify-between">
              <p className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                {label}
              </p>
              {label === 'Active tokens' && value > 0 ? (
                <Badge variant="secondary">{value}</Badge>
              ) : null}
            </div>
            <p className="mt-3 text-2xl font-bold tracking-tight">{value}</p>
            <p className="mt-1 text-xs text-muted-foreground">{description}</p>
          </CardContent>
        </Card>
      ))}
    </section>
  )
}
