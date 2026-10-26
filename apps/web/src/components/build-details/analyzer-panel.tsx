import { Link } from '@tanstack/react-router'
import type {
  AnalyzerDiagnostic,
  AnalyzerSeverity,
  BuildAnalysisResponse,
} from '@oore/client/models'

import { Alert, AlertDescription } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/components/ui/card'
import { Skeleton } from '@/components/ui/skeleton'
import { Row } from '@/components/layout/row'
import { Text } from '@/components/layout/text'
import {
  analyzerTitle,
  formatCountChange,
  plural,
  totalDiagnostics,
} from '@/lib/analyzer-format'

const SEVERITY_LABEL = {
  error: 'Error',
  warning: 'Warning',
  info: 'Info',
} satisfies Record<AnalyzerSeverity, string>

const SEVERITY_BADGE = {
  error: 'destructive',
  warning: 'warning',
  info: 'outline',
} as const satisfies Record<AnalyzerSeverity, string>

function DiagnosticRow({ diagnostic }: { diagnostic: AnalyzerDiagnostic }) {
  return (
    <li className="space-y-1 py-3 first:pt-0 last:pb-0">
      <div className="flex flex-wrap items-start gap-2">
        <Badge variant={SEVERITY_BADGE[diagnostic.severity]}>
          {SEVERITY_LABEL[diagnostic.severity]}
        </Badge>
        <p className="min-w-0 flex-1 text-sm wrap-break-word">
          {diagnostic.message}
        </p>
      </div>
      <Text variant="mono" className="wrap-break-word">
        {diagnostic.file
          ? `${diagnostic.file}:${diagnostic.line}:${diagnostic.column} · `
          : null}
        {diagnostic.code}
      </Text>
    </li>
  )
}

export function AnalyzerPanel({
  data,
  error,
  isLoading,
  onRetry,
}: {
  data: BuildAnalysisResponse | undefined
  error: Error | null
  isLoading: boolean
  onRetry: () => void
}) {
  if (isLoading) return <Skeleton className="h-48 w-full" />
  if (error) {
    return (
      <Alert>
        <AlertDescription className="flex flex-wrap items-center justify-between gap-3">
          Could not load the analyzer results.
          <Button variant="outline" size="sm" onClick={onRetry}>
            Retry
          </Button>
        </AlertDescription>
      </Alert>
    )
  }
  const analysis = data?.analysis
  if (!analysis) {
    return (
      <Text variant="muted">This build did not report analyzer results.</Text>
    )
  }
  const { counts, diagnostics } = analysis
  const baseline = data.baseline
  const hidden = totalDiagnostics(counts) - diagnostics.length
  const severities = [
    { key: 'errors', one: 'error', many: 'errors', variant: 'destructive' },
    { key: 'warnings', one: 'warning', many: 'warnings', variant: 'warning' },
    { key: 'infos', one: 'info', many: 'infos', variant: 'outline' },
  ] as const

  return (
    <Card size="sm">
      <CardHeader className="gap-2">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <CardTitle>{analyzerTitle(counts)}</CardTitle>
          <Row gap={1.5} className="flex-wrap">
            {severities.map(({ key, one, many, variant }) => (
              <Badge
                key={key}
                variant={counts[key] > 0 ? variant : 'outline'}
                title={
                  baseline
                    ? `${baseline.counts[key]} in build #${baseline.build_number}`
                    : undefined
                }
              >
                {plural(counts[key], one, many)}
                {baseline
                  ? ` · ${formatCountChange(counts[key], baseline.counts[key])}`
                  : null}
              </Badge>
            ))}
          </Row>
        </div>
        {baseline ? (
          <CardDescription>
            <span className="font-mono">dart analyze</span> compared with{' '}
            <Link
              to="/builds/$buildId"
              params={{ buildId: baseline.build_id }}
              search={{ tab: 'analyzer' }}
              className="underline underline-offset-2"
            >
              build #{baseline.build_number}
            </Link>{' '}
            on <span className="font-mono">{baseline.branch}</span>
          </CardDescription>
        ) : (
          <CardDescription>
            <span className="font-mono">dart analyze</span>
            {data.base_branch ? (
              <>
                {' '}
                · No earlier successful build on{' '}
                <span className="font-mono">{data.base_branch}</span> reported
                an analysis to compare with
              </>
            ) : null}
          </CardDescription>
        )}
      </CardHeader>
      {diagnostics.length > 0 ? (
        <CardContent>
          <ul className="divide-y">
            {diagnostics.map((diagnostic, index) => (
              <DiagnosticRow
                key={`${diagnostic.file}:${diagnostic.line}:${diagnostic.column}:${diagnostic.code}:${index}`}
                diagnostic={diagnostic}
              />
            ))}
          </ul>
          {hidden > 0 ? (
            <Text variant="caption" className="mt-4">
              {hidden === 1
                ? '1 more diagnostic is not shown.'
                : `${hidden} more diagnostics are not shown.`}{' '}
              Run <span className="font-mono">dart analyze</span> on the commit
              to see them all.
            </Text>
          ) : null}
        </CardContent>
      ) : null}
    </Card>
  )
}
