import { useRef } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import { File01Icon } from '@hugeicons/core-free-icons'

import { Button } from '@/components/ui/button'
import { Label } from '@/components/ui/label'
import { cn } from '@/lib/utils'

/**
 * A credential file picker that says which file the pipeline will use: the
 * one just chosen (saved with the form), the one already stored on the
 * server, or none. Replaces the browser's "Choose File / No file chosen"
 * control, which cannot show a stored file.
 */
export function FileField({
  accept,
  className,
  description,
  emptyLabel = 'No file stored',
  error,
  file,
  id,
  label,
  labelClassName,
  onFileChange,
  storedName,
}: {
  accept: string
  className?: string
  description?: React.ReactNode
  emptyLabel?: string
  error?: string
  file: File | null
  id: string
  label: React.ReactNode
  labelClassName?: string
  onFileChange: (file: File | null) => void
  /** Name of the file already stored, or `true` when its name is unknown. */
  storedName?: string | true | null
}) {
  const inputRef = useRef<HTMLInputElement>(null)
  const descriptionId = `${id}-description`
  const errorId = `${id}-error`
  const hasStored = !!storedName

  function clearSelection() {
    if (inputRef.current) inputRef.current.value = ''
    onFileChange(null)
  }

  return (
    <div className={cn('flex flex-col gap-2', className)}>
      <Label id={`${id}-label`} htmlFor={id} className={labelClassName}>
        {label}
      </Label>
      <div
        className={cn(
          'flex flex-wrap items-center gap-x-3 gap-y-2 rounded-2xl border border-transparent bg-input/50 py-1.5 pr-1.5 pl-3',
          error && 'border-destructive ring-3 ring-destructive/20',
        )}
      >
        <HugeiconsIcon
          icon={File01Icon}
          className="size-4 shrink-0 text-muted-foreground"
          aria-hidden
        />
        <p
          id={`${id}-status`}
          className="min-w-0 flex-1 truncate text-sm"
          aria-live="polite"
        >
          {file ? (
            <>
              <span className="font-mono">{file.name}</span>
              <span className="text-muted-foreground">
                {' '}
                · saves with the pipeline
              </span>
            </>
          ) : hasStored ? (
            <>
              <span className="font-mono">
                {storedName === true ? 'Stored file' : storedName}
              </span>
              <span className="text-muted-foreground"> · stored</span>
            </>
          ) : (
            <span className="text-muted-foreground">{emptyLabel}</span>
          )}
        </p>
        <div className="flex shrink-0 gap-1">
          {file ? (
            <Button
              type="button"
              variant="ghost"
              size="sm"
              aria-describedby={`${id}-label`}
              onClick={clearSelection}
            >
              {hasStored ? 'Keep stored file' : 'Remove'}
            </Button>
          ) : null}
          <Button
            type="button"
            variant="outline"
            size="sm"
            aria-describedby={`${id}-label ${id}-status`}
            onClick={() => inputRef.current?.click()}
          >
            {file || hasStored ? 'Replace' : 'Choose file'}
          </Button>
        </div>
        <input
          ref={inputRef}
          id={id}
          type="file"
          accept={accept}
          className="sr-only"
          // The Replace button is the keyboard control; the label still
          // opens the picker on click.
          tabIndex={-1}
          aria-invalid={!!error}
          aria-describedby={
            [description ? descriptionId : null, error ? errorId : null]
              .filter(Boolean)
              .join(' ') || undefined
          }
          onChange={(event) => onFileChange(event.target.files?.[0] ?? null)}
        />
      </div>
      {description ? (
        <p id={descriptionId} className="text-sm text-muted-foreground">
          {description}
        </p>
      ) : null}
      {error ? (
        <p id={errorId} className="text-sm text-destructive">
          {error}
        </p>
      ) : null}
    </div>
  )
}
