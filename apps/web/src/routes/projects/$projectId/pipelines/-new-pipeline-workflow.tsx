import { SettingsSection } from '@/components/settings/settings-section'
import { Item, ItemContent, ItemTitle } from '@/components/ui/item'
import { RadioGroup, RadioGroupItem } from '@/components/ui/radio-group'

// Base UI radios treat an empty value as unselected.
const NONE = '__none__'

/**
 * Default workflow, asked only when the picked file defines several: the
 * workflow triggers start and Run build picks first.
 */
export function NewPipelineWorkflow({
  onChange,
  path,
  value,
  workflows,
}: {
  onChange: (workflow: string) => void
  path: string
  value: string
  workflows: Array<string>
}) {
  const options = [
    ...workflows.map((name) => ({ key: name, label: name })),
    { key: NONE, label: 'None, pick one each run' },
  ]
  return (
    <SettingsSection
      title="Default workflow"
      hint={`${path} defines several workflows. Triggers start the default; Run build can pick another.`}
    >
      <RadioGroup
        aria-label="Default workflow"
        value={value || NONE}
        onValueChange={(next) => onChange(next === NONE ? '' : String(next))}
        className="sm:grid-cols-2 lg:grid-cols-3"
      >
        {options.map((option) => (
          <Item
            key={option.key}
            variant="outline"
            size="sm"
            render={
              <label
                htmlFor={`new-pipeline-workflow-${option.key}`}
                className="cursor-pointer"
              />
            }
            className="has-data-checked:border-primary has-data-checked:bg-accent"
          >
            <RadioGroupItem
              id={`new-pipeline-workflow-${option.key}`}
              value={option.key}
            />
            <ItemContent>
              <ItemTitle
                className={option.key === NONE ? undefined : 'font-mono'}
              >
                {option.label}
              </ItemTitle>
            </ItemContent>
          </Item>
        ))}
      </RadioGroup>
    </SettingsSection>
  )
}
