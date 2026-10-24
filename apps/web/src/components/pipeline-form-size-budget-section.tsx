import { useFieldArray, useFormContext } from 'react-hook-form'
import { HugeiconsIcon } from '@hugeicons/react'
import { Add01Icon, Delete02Icon } from '@hugeicons/core-free-icons'

import type { PipelineFormValues } from '@/lib/pipeline-schema'
import { MAX_SIZE_BUDGET_RULES } from '@/lib/pipeline-schema'
import { PipelineFormSection } from '@/components/pipeline-form-section'
import { Button } from '@/components/ui/button'
import {
  FormControl,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
} from '@/components/ui/form'
import { Input } from '@/components/ui/input'
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Stack } from '@/components/layout/stack'
import { Text } from '@/components/layout/text'
import { cn } from '@/lib/utils'

const SIZE_BUDGET_DOCS =
  'https://docs.oore.build/build/run/pull-request-checks#size-budget'

const MODE_OPTIONS = {
  warn: 'Warn on the check and keep its result',
  fail: 'Fail the check',
} satisfies Record<PipelineFormValues['size_budget_mode'], string>

type LimitField = 'max_mb' | 'max_increase_mb' | 'max_increase_percent'

const LIMIT_FIELDS = [
  ['max_mb', 'Max size (MB)', 'No limit'],
  ['max_increase_mb', 'Max increase (MB)', 'No limit'],
  ['max_increase_percent', 'Max increase (%)', 'No limit'],
] as const satisfies ReadonlyArray<readonly [LimitField, string, string]>

/**
 * Size budget: limits per artifact kind that a pull request check enforces
 * once a build succeeds. The budget never fails the build itself.
 */
export function PipelineSizeBudgetSection() {
  const form = useFormContext<PipelineFormValues>()
  const rules = useFieldArray({
    control: form.control,
    name: 'size_budget_rules',
  })

  return (
    <PipelineFormSection
      id="size-budget"
      title="Size budget"
      hint="An artifact over budget can fail its pull request check, never the build."
      actions={
        <Button
          type="button"
          variant="outline"
          size="sm"
          disabled={rules.fields.length >= MAX_SIZE_BUDGET_RULES}
          onClick={() =>
            rules.append({
              kind: rules.fields.length === 0 ? 'aab' : '',
              max_mb: '',
              max_increase_mb: '',
              max_increase_percent: '',
            })
          }
        >
          <HugeiconsIcon icon={Add01Icon} />
          Add rule
        </Button>
      }
    >
      <FormField
        control={form.control}
        name="size_budget_mode"
        render={({ field }) => (
          <FormItem>
            <FormLabel>When an artifact is over budget</FormLabel>
            <Select
              value={field.value}
              onValueChange={field.onChange}
              items={MODE_OPTIONS}
            >
              <FormControl>
                <SelectTrigger
                  className="w-full"
                  aria-label="When an artifact is over budget"
                >
                  <SelectValue />
                </SelectTrigger>
              </FormControl>
              <SelectContent>
                <SelectGroup>
                  {Object.entries(MODE_OPTIONS).map(([key, label]) => (
                    <SelectItem key={key} value={key}>
                      {label}
                    </SelectItem>
                  ))}
                </SelectGroup>
              </SelectContent>
            </Select>
            <FormMessage />
          </FormItem>
        )}
      />

      {rules.fields.length === 0 ? (
        <Text variant="muted">
          No rules. Pull request checks show artifact sizes without a budget.
        </Text>
      ) : (
        <Stack gap={3}>
          {rules.fields.map((rule, index) => (
            <div
              key={rule.id}
              className={cn(
                'grid grid-cols-2 gap-2 sm:grid-cols-[6rem_repeat(3,minmax(0,1fr))_auto] sm:items-start',
                // Each rule repeats its labels on phones, so a rule line
                // tells them apart.
                index > 0 && 'border-t pt-3 sm:border-t-0 sm:pt-0',
              )}
            >
              <FormField
                control={form.control}
                name={`size_budget_rules.${index}.kind`}
                render={({ field }) => (
                  <FormItem className="col-span-2 sm:col-span-1">
                    <FormLabel
                      className={index === 0 ? undefined : 'sm:sr-only'}
                    >
                      Kind
                    </FormLabel>
                    <FormControl>
                      <Input
                        placeholder="aab"
                        maxLength={16}
                        className="font-mono"
                        aria-label={`Rule ${index + 1} kind`}
                        {...field}
                      />
                    </FormControl>
                    <FormMessage />
                  </FormItem>
                )}
              />
              {LIMIT_FIELDS.map(([name, label, placeholder]) => (
                <FormField
                  key={name}
                  control={form.control}
                  name={`size_budget_rules.${index}.${name}`}
                  render={({ field }) => (
                    <FormItem>
                      <FormLabel
                        className={index === 0 ? undefined : 'sm:sr-only'}
                      >
                        {label}
                      </FormLabel>
                      <FormControl>
                        <Input
                          type="number"
                          inputMode="decimal"
                          min={0}
                          step="any"
                          placeholder={placeholder}
                          aria-label={`Rule ${index + 1} ${label}`}
                          {...field}
                          value={field.value ?? ''}
                        />
                      </FormControl>
                      <FormMessage />
                    </FormItem>
                  )}
                />
              ))}
              <Button
                type="button"
                variant="ghost"
                size="icon"
                className={index === 0 ? 'mt-6' : 'mt-6 sm:mt-0'}
                aria-label={`Remove rule ${index + 1}`}
                onClick={() => rules.remove(index)}
              >
                <HugeiconsIcon icon={Delete02Icon} />
              </Button>
            </div>
          ))}
        </Stack>
      )}
      <Text variant="muted">
        A kind is a file extension, such as{' '}
        <span className="font-mono">aab</span>,{' '}
        <span className="font-mono">apk</span> or{' '}
        <span className="font-mono">ipa</span>. Increases compare with the last
        successful build on the base branch; without one, only the max size
        applies.{' '}
        <a
          href={SIZE_BUDGET_DOCS}
          target="_blank"
          rel="noopener noreferrer"
          className="underline underline-offset-4"
        >
          Learn more
        </a>
      </Text>
    </PipelineFormSection>
  )
}
