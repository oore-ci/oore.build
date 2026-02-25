import type { JSX } from 'solid-js'
import { cn } from '@/lib/utils'

interface FormFieldProps {
  label?: string
  description?: string
  error?: string | null
  class?: string
  children: JSX.Element
}

export function FormField(props: FormFieldProps) {
  return (
    <div class={cn('space-y-1', props.class)}>
      {props.label ? (
        <label class="text-xs font-medium text-muted-foreground">{props.label}</label>
      ) : null}
      {props.children}
      {props.description ? (
        <p class="text-xs text-muted-foreground">{props.description}</p>
      ) : null}
      {props.error ? <FormError>{props.error}</FormError> : null}
    </div>
  )
}

interface FormErrorProps {
  children: JSX.Element
}

export function FormError(props: FormErrorProps) {
  return <p class="text-xs text-destructive">{props.children}</p>
}
