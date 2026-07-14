import { useFormContext } from 'react-hook-form'

import type { PipelineFormValues } from '@/lib/pipeline-schema'
import { PipelineFormSectionHeader } from '@/components/pipeline-form-section-header'
import SetupHint from '@/components/setup-hint'
import { Card, CardContent, CardHeader } from '@/components/ui/card'
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
import { Textarea } from '@/components/ui/textarea'

interface SectionProps {
  hidden: boolean
  onOpenChange: (open: boolean) => void
  open: boolean
}

export function PipelineEnvironmentSection({
  envVarCount,
  hidden,
  onOpenChange,
  open,
}: SectionProps & { envVarCount: number }) {
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
              title="Environment Variables"
              summary={
                envVarCount > 0
                  ? `${envVarCount} env var${envVarCount !== 1 ? 's' : ''} configured`
                  : 'None configured'
              }
              open={open}
            />
          </CardHeader>
        </CollapsibleTrigger>
        <CollapsibleContent>
          <CardContent className="space-y-4">
            <FormField
              control={form.control}
              name="env_vars"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>Environment variables</FormLabel>
                  <FormControl>
                    <Textarea
                      placeholder={
                        'API_BASE_URL=https://api.example.com\nAPP_FLAVOR=dev'
                      }
                      className="font-mono"
                      rows={6}
                      {...field}
                    />
                  </FormControl>
                  <p className="text-xs text-muted-foreground">
                    One KEY=VALUE per line. Available in all build steps.
                  </p>
                  <SetupHint
                    title="Built-in CI variables"
                    items={[
                      <span key="built-in-variables">
                        Oore also injects <code>PROJECT_ID</code>,{' '}
                        <code>PIPELINE_ID</code>, <code>BUILD_ID</code>,{' '}
                        <code>PROJECT_BUILD_NUMBER</code>,{' '}
                        <code>BUILD_NUMBER</code>, <code>CI=true</code>, and
                        branch/commit values when present.
                      </span>,
                      'Use this section for app-specific values such as API_BASE_URL, APP_FLAVOR, SENTRY_DSN, or feature flags.',
                    ]}
                  />
                  <FormMessage />
                </FormItem>
              )}
            />
          </CardContent>
        </CollapsibleContent>
      </Card>
    </Collapsible>
  )
}

export function PipelineArtifactsSection({
  artifactPatterns,
  hidden,
  onOpenChange,
  open,
}: SectionProps & { artifactPatterns: Array<string> }) {
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
              title="Artifacts"
              summary={
                artifactPatterns.length > 0
                  ? artifactPatterns.join(', ')
                  : 'Using platform defaults'
              }
              open={open}
            />
          </CardHeader>
        </CollapsibleTrigger>
        <CollapsibleContent>
          <CardContent className="space-y-4">
            <FormField
              control={form.control}
              name="artifact_patterns"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>Artifact patterns</FormLabel>
                  <FormControl>
                    <Textarea
                      placeholder={'One pattern per line\n*.apk\n*.ipa'}
                      className="font-mono"
                      rows={4}
                      {...field}
                    />
                  </FormControl>
                  <FormMessage />
                </FormItem>
              )}
            />
          </CardContent>
        </CollapsibleContent>
      </Card>
    </Collapsible>
  )
}
