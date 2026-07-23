import { zodResolver } from '@hookform/resolvers/zod'
import { useNavigate } from '@tanstack/react-router'
import { useForm } from 'react-hook-form'
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

const demoLoginSchema = z.object({
  email: z.email('Enter one of the demo email addresses.'),
  password: z.string().min(1, 'Enter the demo password.'),
})

type DemoLoginValues = z.infer<typeof demoLoginSchema>

export default function DemoLoginForm() {
  const navigate = useNavigate()
  const setAuth = useAuthStore((state) => state.setAuth)
  const form = useForm<DemoLoginValues>({
    resolver: zodResolver(demoLoginSchema),
    defaultValues: {
      email: DEMO_PERSONAS[0].email,
      password: DEMO_PASSWORD,
    },
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
    void navigate({ to: '/', replace: true })
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
              Sign in as{' '}
              {form.watch('email').split('+')[1]?.split('@')[0] ?? 'user'}
            </Button>
          </form>
        </Form>

        <div className="space-y-2 border-t pt-4">
          <p className="text-xs font-medium text-muted-foreground">
            Demo accounts
          </p>
          <div className="grid grid-cols-2 gap-2">
            {DEMO_PERSONAS.map((persona) => (
              <Button
                key={persona.role}
                type="button"
                variant="outline"
                className="h-auto justify-start px-3 py-2 text-left"
                onClick={() => {
                  form.setValue('email', persona.email, {
                    shouldValidate: true,
                  })
                  form.setValue('password', DEMO_PASSWORD)
                  form.clearErrors()
                }}
              >
                <span>
                  <span className="block capitalize">
                    {persona.role.replace('_', ' ')}
                  </span>
                  <span className="block text-xs font-normal text-muted-foreground">
                    {persona.displayName}
                  </span>
                </span>
              </Button>
            ))}
          </div>
          <p className="text-xs text-muted-foreground">
            All accounts use password <code>{DEMO_PASSWORD}</code>.
          </p>
        </div>
      </CardContent>
    </Card>
  )
}
