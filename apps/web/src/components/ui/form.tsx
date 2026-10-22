import * as React from 'react'
import {
  Controller,
  FormProvider,
  useFormContext,
  useFormState,
} from 'react-hook-form'
import type { ControllerProps, FieldPath, FieldValues } from 'react-hook-form'

import { cn } from '@/lib/utils'
import { Label } from '@/components/ui/label'

const Form = FormProvider

type FormFieldContextValue<
  TFieldValues extends FieldValues = FieldValues,
  TName extends FieldPath<TFieldValues> = FieldPath<TFieldValues>,
> = {
  name: TName
}

const FormFieldContext = React.createContext<FormFieldContextValue | null>(null)

function FormField<
  TFieldValues extends FieldValues = FieldValues,
  TName extends FieldPath<TFieldValues> = FieldPath<TFieldValues>,
>({ ...props }: ControllerProps<TFieldValues, TName>) {
  const contextValue = React.useMemo(() => ({ name: props.name }), [props.name])

  return (
    <FormFieldContext.Provider value={contextValue}>
      <Controller {...props} />
    </FormFieldContext.Provider>
  )
}

function useFormField() {
  const fieldContext = React.useContext(FormFieldContext)
  const itemContext = React.useContext(FormItemContext)
  const { getFieldState } = useFormContext()
  // Subscribe to this field's state directly. Reading `formState` from the
  // context does not re-render memoised (React Compiler) sections when a
  // submit adds errors, so the message would stay hidden until a later blur.
  const formState = useFormState({ name: fieldContext?.name })

  if (!fieldContext || !itemContext) {
    throw new Error(
      'Form fields must be rendered inside FormField and FormItem',
    )
  }

  const fieldState = getFieldState(fieldContext.name, formState)
  const { id } = itemContext

  return {
    id,
    name: fieldContext.name,
    formItemId: `${id}-form-item`,
    formDescriptionId: `${id}-form-item-description`,
    formMessageId: `${id}-form-item-message`,
    ...fieldState,
  }
}

type FormItemContextValue = {
  id: string
}

const FormItemContext = React.createContext<FormItemContextValue | null>(null)

function FormItem({ className, ...props }: React.ComponentProps<'div'>) {
  const id = React.useId()
  const contextValue = React.useMemo(() => ({ id }), [id])

  return (
    <FormItemContext.Provider value={contextValue}>
      <div
        data-slot="form-item"
        className={cn('space-y-2', className)}
        {...props}
      />
    </FormItemContext.Provider>
  )
}

function FormLabel({
  className,
  ...props
}: React.ComponentProps<typeof Label>) {
  const { error, formItemId } = useFormField()

  return (
    <Label
      className={cn(error && 'text-destructive', className)}
      htmlFor={formItemId}
      {...props}
    />
  )
}

interface FormControlChildProps {
  'aria-describedby'?: string
  'aria-invalid'?: boolean
  id?: string
}

function FormControl({
  children,
}: {
  children: React.ReactElement<FormControlChildProps>
}) {
  const { error, formItemId, formDescriptionId, formMessageId } = useFormField()

  return React.cloneElement(children, {
    id: formItemId,
    'aria-describedby': !error
      ? formDescriptionId
      : `${formDescriptionId} ${formMessageId}`,
    'aria-invalid': !!error,
  })
}

function FormDescription({ className, ...props }: React.ComponentProps<'p'>) {
  const { formDescriptionId } = useFormField()

  return (
    <p
      data-slot="form-description"
      id={formDescriptionId}
      className={cn('text-sm text-muted-foreground', className)}
      {...props}
    />
  )
}

function FormMessage({
  className,
  children,
  ...props
}: React.ComponentProps<'p'>) {
  const { error, formMessageId } = useFormField()
  const body = error ? String(error.message ?? '') : children

  if (!body) {
    return null
  }

  return (
    <p
      data-slot="form-message"
      id={formMessageId}
      className={cn('text-sm text-destructive', className)}
      {...props}
    >
      {body}
    </p>
  )
}

export {
  useFormField,
  Form,
  FormItem,
  FormLabel,
  FormControl,
  FormDescription,
  FormMessage,
  FormField,
}
