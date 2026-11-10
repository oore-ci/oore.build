import { HugeiconsIcon } from '@hugeicons/react'
import {
  AlertCircleIcon,
  InformationCircleIcon,
} from '@hugeicons/core-free-icons'

import type { BuildFailureHint } from '@oore/client/models'

import type { FailureLine } from '@/components/terminal-log-viewer/log-model'
import type { BuildStateSummary } from '@/lib/build-state-description'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'

/** Plain text with `code` spans, as failure hints write commands. */
function WithCode({ text }: { text: string }) {
  return text.split('`').map((part, index) =>
    index % 2 ? (
      <code key={index} className="font-mono text-xs text-foreground">
        {part}
      </code>
    ) : (
      part
    ),
  )
}

function LineChip({
  text,
  label,
  ariaLabel,
  onShow,
}: {
  text: string
  label: string
  ariaLabel: string
  onShow: () => void
}) {
  return (
    // oxlint-disable-next-line layout/max-class-utilities -- failure line chip
    <div className="flex min-w-0 items-start gap-2 rounded-lg border bg-muted/40 py-1 pr-1 pl-2.5">
      <code className="min-w-0 flex-1 py-0.5 font-mono text-xs wrap-break-word whitespace-pre-wrap text-foreground">
        {text}
      </code>
      <Button
        variant="ghost"
        size="xs"
        className="shrink-0"
        aria-label={ariaLabel}
        onClick={onShow}
      >
        {label}
      </Button>
    </div>
  )
}

export function BuildStateAlert({
  summary,
  failureLine,
  hint,
  nextStep,
  details,
  onShowFailureLine,
  onShowHintLine,
}: {
  summary: BuildStateSummary
  failureLine: FailureLine | null
  /** The daemon's reading of the log: the line that explains the failure
   * and its likely cause. It replaces the line the page found. */
  hint?: BuildFailureHint | null
  onShowHintLine?: (hint: BuildFailureHint) => void
  nextStep?: React.ReactNode
  /** Facts the reader needs to act, such as who asked for an approval. */
  details?: Array<{ label: string; value: React.ReactNode }>
  onShowFailureLine: () => void
}) {
  const destructive = summary.tone === 'destructive'
  return (
    <Alert variant={destructive ? 'destructive' : 'default'}>
      <HugeiconsIcon
        icon={destructive ? AlertCircleIcon : InformationCircleIcon}
        size={16}
      />
      <AlertTitle role="status">{summary.title}</AlertTitle>
      <AlertDescription className="flex min-w-0 flex-col gap-2">
        {hint ? (
          <>
            <LineChip
              text={hint.line}
              label="Show in Logs"
              ariaLabel="Show this line in Logs"
              onShow={() => onShowHintLine?.(hint)}
            />
            <div>
              <span className="font-medium text-foreground">Likely cause:</span>{' '}
              <WithCode text={hint.cause} />
              {hint.fix ? ' ' : ''}
              {hint.fix ? <WithCode text={hint.fix} /> : null}
            </div>
            {nextStep ? <div>{nextStep}</div> : null}
          </>
        ) : failureLine ? (
          <>
            <LineChip
              text={failureLine.text}
              label={`Line ${failureLine.lineNumber}`}
              ariaLabel={`Show line ${failureLine.lineNumber} in Logs`}
              onShow={onShowFailureLine}
            />
            {nextStep ? <div>{nextStep}</div> : null}
          </>
        ) : summary.description || nextStep ? (
          <div>
            {summary.description} {nextStep}
          </div>
        ) : null}
        {details?.length ? (
          <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-1 text-sm sm:grid-cols-[repeat(2,auto_minmax(0,1fr))]">
            {details.map((detail) => (
              <div key={detail.label} className="contents">
                <dt className="text-muted-foreground">{detail.label}</dt>
                <dd className="min-w-0 truncate text-foreground">
                  {detail.value}
                </dd>
              </div>
            ))}
          </dl>
        ) : null}
      </AlertDescription>
    </Alert>
  )
}
