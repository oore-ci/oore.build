import type { ReactNode } from 'react'

import { Button } from '@/components/ui/button'
import { Spinner } from '@/components/ui/spinner'
import { cn } from '@/lib/utils'

/**
 * Save and Cancel at the end of a form, in one order everywhere: secondary
 * actions, then Cancel, then the primary submit.
 *
 * `sticky` is for long forms only. Pass a boolean (usually the form's dirty
 * state) and the row gets a surface at card radius that pins to the bottom
 * while there are unsaved changes; the geometry is constant in both states so
 * nothing jumps when the form becomes dirty.
 */
export function FormActions({
  cancelLabel = 'Cancel',
  children,
  className,
  disabled = false,
  disabledReason,
  isPending = false,
  onCancel,
  pendingLabel = 'Saving...',
  sticky,
  submitLabel,
}: {
  cancelLabel?: string
  children?: ReactNode
  className?: string
  disabled?: boolean
  disabledReason?: string
  isPending?: boolean
  onCancel?: () => void
  pendingLabel?: string
  sticky?: boolean
  submitLabel: string
}) {
  const chassis = sticky !== undefined
  return (
    <div
      data-slot="form-actions"
      data-sticky={sticky ? '' : undefined}
      className={cn(
        'flex flex-wrap items-center justify-end gap-2',
        chassis &&
          'rounded-[min(var(--radius-4xl),24px)] border border-transparent p-3 transition-colors',
        sticky &&
          'sticky bottom-0 z-30 border-border bg-surface/95 shadow-xs backdrop-blur supports-backdrop-filter:bg-surface/80',
        className,
      )}
    >
      {children}
      {onCancel ? (
        <Button type="button" variant="outline" onClick={onCancel}>
          {cancelLabel}
        </Button>
      ) : null}
      <Button
        type="submit"
        disabled={isPending || disabled}
        title={disabled ? disabledReason : undefined}
      >
        {isPending ? <Spinner data-icon="inline-start" /> : null}
        {isPending ? pendingLabel : submitLabel}
      </Button>
    </div>
  )
}
