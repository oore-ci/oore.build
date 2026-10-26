import { zodResolver } from '@hookform/resolvers/zod'
import { useNavigate } from '@tanstack/react-router'
import { useForm, useWatch } from 'react-hook-form'
import * as z from 'zod'
import { DEMO_PERSONAS, authenticateDemoUser } from '@/demo/personas'
import { DEMO_PASSWORD } from '@/demo/seed'
import { queryClient } from '@/lib/query-client'
import { useAuthStore } from '@/stores/auth-store'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { Card, CardContent } from '@/components/ui/card'
import {
  Form,
  FormControl,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
} from '@/components/ui/form'
import { Input } from '@/components/ui/input'
import { Separator } from '@/components/ui/separator'
import { ROLE_LABELS } from '@/routes/settings/-user-role-labels'
import { Text } from '@/components/layout/text'
import { textVariants } from '@/lib/recipes'
import { cn } from '@/lib/utils'

const demoLoginSchema = z.object({
  email: z.email('Enter one of the demo email addresses.'),
  password: z.string().min(1, 'Enter the demo password.'),
})

type DemoLoginValues = z.infer<typeof demoLoginSchema>

export default function DemoLoginForm({ redirectTo }: { redirectTo: string }) {
  const navigate = useNavigate()
  const setAuth = useAuthStore((state) => state.setAuth)
  const form = useForm<DemoLoginValues>({
    resolver: zodResolver(demoLoginSchema),
    defaultValues: {
      email: DEMO_PERSONAS[0].email,
      password: DEMO_PASSWORD,
    },
  })
  const watchedEmail = useWatch({
    control: form.control,
    name: 'email',
  })
  const invalidCredentials = form.formState.errors.root?.message

  function signIn(values: DemoLoginValues) {
    const persona = authenticateDemoUser(values.email, values.password)
    if (!persona) {
      form.setError('root', { message: 'Invalid demo email or password.' })
      return
    }

    queryClient.clear()
    setAuth(
      persona.token,
      4102444800,
      {
        email: persona.email,
        oidc_subject: `demo::${persona.role}`,
        user_id: persona.userId,
        role: persona.role,
      },
      'local',
    )
    void navigate({ href: redirectTo, replace: true })
  }

  return (
    <Card>
      <CardContent className="space-y-5">
        <Form {...form}>
          <form className="space-y-4" onSubmit={form.handleSubmit(signIn)}>
            <FormField
              control={form.control}
              name="email"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>Email</FormLabel>
                  <FormControl>
                    <Input {...field} autoComplete="email" />
                  </FormControl>
                  <FormMessage />
                </FormItem>
              )}
            />
            <FormField
              control={form.control}
              name="password"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>Password</FormLabel>
                  <FormControl>
                    <Input
                      {...field}
                      type="password"
                      autoComplete="current-password"
                    />
                  </FormControl>
                  <FormMessage />
                </FormItem>
              )}
            />
            {invalidCredentials ? (
              <Alert variant="destructive">
                <AlertDescription>{invalidCredentials}</AlertDescription>
              </Alert>
            ) : null}
            <Button type="submit" className="w-full">
              Sign in
            </Button>
          </form>
        </Form>

        <div className="space-y-2">
          <Separator />
          <Text variant="caption" className="font-medium">
            Demo accounts
          </Text>
          <div className="grid grid-cols-2 gap-2">
            {DEMO_PERSONAS.map((persona) => (
              <Button
                key={persona.role}
                type="button"
                variant="outline"
                aria-pressed={watchedEmail === persona.email}
                className="h-auto justify-start px-3 py-2 text-left aria-pressed:border-primary aria-pressed:bg-accent"
                onClick={() => {
                  form.setValue('email', persona.email, {
                    shouldValidate: true,
                  })
                  form.setValue('password', DEMO_PASSWORD)
                  form.clearErrors()
                }}
              >
                <span>
                  <span className="block">{ROLE_LABELS[persona.role]}</span>
                  <span
                    className={cn(
                      textVariants({ variant: 'caption' }),
                      'block font-normal',
                    )}
                  >
                    {persona.displayName}
                  </span>
                </span>
              </Button>
            ))}
          </div>
          <Text variant="caption">
            All accounts use password <code>{DEMO_PASSWORD}</code>.
          </Text>
        </div>
      </CardContent>
    </Card>
  )
}
