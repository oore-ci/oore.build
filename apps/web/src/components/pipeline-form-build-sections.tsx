import { useFormContext } from 'react-hook-form'

import type { PipelineFormValues } from '@/lib/pipeline-schema'
import { PipelineFormSectionHeader } from '@/components/pipeline-form-section-header'
import { Card, CardContent, CardHeader } from '@/components/ui/card'
import { Checkbox } from '@/components/ui/checkbox'
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible'
import {
  FormControl,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
} from '@/components/ui/form'
import { Input } from '@/components/ui/input'
import { Separator } from '@/components/ui/separator'
import { Textarea } from '@/components/ui/textarea'

interface SectionProps {
  hidden: boolean
  onOpenChange: (open: boolean) => void
  open: boolean
}

const PLATFORM_ARGUMENT_FIELDS = [
  [
    'android_build_args',
    'Android build args',
    '--dart-define-from-file=config/dev.json\n--obfuscate',
  ],
  [
    'ios_build_args',
    'iOS build args',
    '--flavor=production\n--build-number=$PROJECT_BUILD_NUMBER',
  ],
  [
    'macos_build_args',
    'macOS build args',
    '--build-number=$PROJECT_BUILD_NUMBER',
  ],
] as const

const PLATFORM_COMMAND_OVERRIDES = [
  [
    'android_command_override',
    'Android command override',
    'flutter build appbundle --release',
  ],
  [
    'ios_command_override',
    'iOS command override',
    'flutter build ios --release --no-codesign',
  ],
  [
    'macos_command_override',
    'macOS command override',
    'flutter build macos --release',
  ],
] as const

export function PipelineCommandsSection({
  commandCount,
  hidden,
  onOpenChange,
  open,
}: SectionProps & { commandCount: number }) {
  const form = useFormContext<PipelineFormValues>()
  const customizationEnabled = form.watch('enable_customization')

  return (
    <Collapsible
      className={hidden ? 'hidden' : undefined}
      open={open}
      onOpenChange={onOpenChange}
    >
      <Card>
        <CollapsibleTrigger className="w-full cursor-pointer">
          <CardHeader>
            <PipelineFormSectionHeader
              title="Build Commands"
              summary={
                commandCount > 0
                  ? `${commandCount} custom command${commandCount !== 1 ? 's' : ''}`
                  : 'Using defaults'
              }
              open={open}
            />
          </CardHeader>
        </CollapsibleTrigger>
        <CollapsibleContent>
          <CardContent className="space-y-4">
            <FormField
              control={form.control}
              name="enable_customization"
              render={({ field }) => (
                <FormItem>
                  <label className="flex items-center gap-2 text-sm font-medium">
                    <Checkbox
                      checked={field.value}
                      onCheckedChange={(checked) => field.onChange(!!checked)}
                    />
                    Enable custom build commands
                  </label>
                </FormItem>
              )}
            />

            {customizationEnabled ? (
              <>
                <CommandField
                  name="pre_build_commands"
                  label="Pre-build commands"
                  placeholder={'One command per line\nexample: dart --version'}
                />
                <CommandField
                  name="build_commands"
                  label="Extra build commands"
                  placeholder={'One command per line\nexample: flutter test'}
                  description="Runs after default platform commands."
                />
                <CommandField
                  name="post_build_commands"
                  label="Post-build commands"
                  placeholder={'One command per line\nexample: ls -la build/'}
                />
              </>
            ) : (
              <p className="text-xs text-muted-foreground">
                Using sensible defaults. Enable to add custom pre/build/post
                commands.
              </p>
            )}
          </CardContent>
        </CollapsibleContent>
      </Card>
    </Collapsible>
  )
}

function CommandField({
  description,
  label,
  name,
  placeholder,
}: {
  description?: string
  label: string
  name: 'pre_build_commands' | 'build_commands' | 'post_build_commands'
  placeholder: string
}) {
  const form = useFormContext<PipelineFormValues>()
  return (
    <FormField
      control={form.control}
      name={name}
      render={({ field }) => (
        <FormItem>
          <FormLabel>{label}</FormLabel>
          <FormControl>
            <Textarea
              placeholder={placeholder}
              className="font-mono"
              {...field}
            />
          </FormControl>
          {description ? (
            <p className="text-xs text-muted-foreground">{description}</p>
          ) : null}
          <FormMessage />
        </FormItem>
      )}
    />
  )
}

export function PipelinePlatformArgsSection({
  hidden,
  onOpenChange,
  open,
}: SectionProps) {
  const form = useFormContext<PipelineFormValues>()

  return (
    <Collapsible
      className={hidden ? 'hidden' : undefined}
      open={open}
      onOpenChange={onOpenChange}
    >
      <Card>
        <CollapsibleTrigger className="w-full cursor-pointer">
          <CardHeader>
            <PipelineFormSectionHeader
              title="Platform Build Args"
              summary="Per-platform args + command overrides"
              open={open}
            />
          </CardHeader>
        </CollapsibleTrigger>
        <CollapsibleContent>
          <CardContent className="space-y-4">
            <p className="text-xs text-muted-foreground">
              Each line is appended to the platform's default Flutter build
              command.
            </p>
            {PLATFORM_ARGUMENT_FIELDS.map(([name, label, placeholder]) => (
              <FormField
                key={name}
                control={form.control}
                name={name}
                render={({ field }) => (
                  <FormItem>
                    <FormLabel>{label}</FormLabel>
                    <FormControl>
                      <Textarea
                        placeholder={placeholder}
                        className="font-mono"
                        {...field}
                      />
                    </FormControl>
                    <FormMessage />
                  </FormItem>
                )}
              />
            ))}
            <Separator />
            <p className="text-sm font-medium">
              Per-platform command override (optional)
            </p>
            <p className="text-xs text-muted-foreground">
              If set, this command fully replaces the default platform command.
            </p>
            {PLATFORM_COMMAND_OVERRIDES.map(([name, label, placeholder]) => (
              <FormField
                key={name}
                control={form.control}
                name={name}
                render={({ field }) => (
                  <FormItem>
                    <FormLabel>{label}</FormLabel>
                    <FormControl>
                      <Input
                        placeholder={placeholder}
                        className="font-mono"
                        {...field}
                      />
                    </FormControl>
                    <FormMessage />
                  </FormItem>
                )}
              />
            ))}
          </CardContent>
        </CollapsibleContent>
      </Card>
    </Collapsible>
  )
}
