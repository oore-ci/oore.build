import { useState } from 'react'
import { createFileRoute, redirect } from '@tanstack/react-router'
import { useForm } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import z from 'zod'
import { toast } from 'sonner'

import type { ApiTokenSummary, CreateApiTokenResponse } from '@/lib/types'
import {
  getActiveInstanceOrRedirect,
  requireAuthOrRedirect,
} from '@/lib/instance-context'
import { getApiErrorMessage } from '@/lib/api'
import { useAuthStore } from '@/stores/auth-store'
import { useHasPermission } from '@/hooks/use-permissions'
import {
  useApiTokens,
  useCreateApiToken,
  useRevokeApiToken,
} from '@/hooks/use-api-tokens'
import { PageMeta } from '@/lib/seo'
import PageLayout from '@/components/page-layout'
import PageHeader from '@/components/page-header'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import {
  Form,
  FormControl,
  FormDescription,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
} from '@/components/ui/form'
import { Input } from '@/components/ui/input'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Skeleton } from '@/components/ui/skeleton'
import { Spinner } from '@/components/ui/spinner'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import ConfirmDialog from '@/components/ConfirmDialog'

export const Route = createFileRoute('/settings/api-tokens')({
  staticData: { breadcrumbLabel: 'API Tokens' },
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireAuthOrRedirect(instance.id)

    const user = useAuthStore.getState().user
    if (
      !user ||
      (user.role !== 'owner' &&
        user.role !== 'admin' &&
        user.role !== 'developer')
    ) {
      throw redirect({ to: '/' })
    }
  },
  component: ApiTokensPage,
})

// ── Helpers ─────────────────────────────────────────────────────

function formatRelativeTime(epochSeconds?: number | null): string {
  if (!epochSeconds) return 'Never'
  const diffSecs = Math.floor(Date.now() / 1000) - epochSeconds
  if (diffSecs < 5) return 'just now'
  if (diffSecs < 60) return `${diffSecs}s ago`
  const mins = Math.floor(diffSecs / 60)
  if (mins < 60) return `${mins}m ago`
  const hrs = Math.floor(mins / 60)
  if (hrs < 24) return `${hrs}h ago`
  const days = Math.floor(hrs / 24)
  return `${days}d ago`
}

function getTokenStatus(
  token: ApiTokenSummary,
): 'active' | 'expired' | 'revoked' {
  if (token.is_revoked) return 'revoked'
  if (token.is_expired) return 'expired'
  return 'active'
}

function getStatusVariant(
  status: 'active' | 'expired' | 'revoked',
): 'success' | 'secondary' | 'destructive' {
  switch (status) {
    case 'active':
      return 'success'
    case 'expired':
      return 'secondary'
    case 'revoked':
      return 'destructive'
  }
}

const ROLE_HIERARCHY: Array<string> = [
  'owner',
  'admin',
  'developer',
  'qa_viewer',
]

const ROLE_LABELS: Record<string, string> = {
  owner: 'Owner',
  admin: 'Admin',
  developer: 'Developer',
  qa_viewer: 'QA Viewer',
}

const EXPIRY_OPTIONS: Record<string, string> = {
  never: 'Never',
  '30': '30 days',
  '90': '90 days',
  '365': '1 year',
}

// ── Create Token Form Schema ────────────────────────────────────

const createTokenSchema = z.object({
  name: z
    .string()
    .trim()
    .min(1, 'Name is required')
    .max(255, 'Name must be at most 255 characters'),
  role: z.string().min(1, 'Role is required'),
  expiry: z.string(),
})

type CreateTokenFormValues = z.infer<typeof createTokenSchema>

// ── Create Token Dialog ─────────────────────────────────────────

interface CreateTokenDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  onCreated: (response: CreateApiTokenResponse) => void
}

function CreateTokenDialog({
  open,
  onOpenChange,
  onCreated,
}: CreateTokenDialogProps) {
  const authUser = useAuthStore((s) => s.user)
  const createMutation = useCreateApiToken()

  const userRoleIndex = ROLE_HIERARCHY.indexOf(authUser?.role ?? 'qa_viewer')
  const availableRoles = ROLE_HIERARCHY.slice(userRoleIndex)

  const form = useForm<CreateTokenFormValues>({
    resolver: zodResolver(createTokenSchema),
    defaultValues: {
      name: '',
      role: authUser?.role ?? 'developer',
      expiry: '90',
    },
    mode: 'onBlur',
  })

  function handleClose(nextOpen: boolean) {
    if (!nextOpen) {
      form.reset()
    }
    onOpenChange(nextOpen)
  }

  function onSubmit(values: CreateTokenFormValues) {
    const expiresAt =
      values.expiry === 'never'
        ? undefined
        : Math.floor(Date.now() / 1000) +
          Number(values.expiry) * 24 * 60 * 60

    createMutation.mutate(
      {
        name: values.name,
        role: values.role,
        expires_at: expiresAt,
      },
      {
        onSuccess: (response) => {
          handleClose(false)
          onCreated(response)
        },
        onError: (err) => {
          toast.error(
            getApiErrorMessage(err, {
              token_limit_reached:
                'You have reached the maximum number of API tokens.',
            }),
          )
        },
      },
    )
  }

  return (
    <Dialog open={open} onOpenChange={handleClose}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Create API Token</DialogTitle>
          <DialogDescription>
            Create a new API token for programmatic access to this instance.
          </DialogDescription>
        </DialogHeader>

        <Form {...form}>
          <form onSubmit={form.handleSubmit(onSubmit)} className="space-y-4">
            <FormField
              control={form.control}
              name="name"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>Name</FormLabel>
                  <FormControl>
                    <Input
                      autoFocus
                      placeholder="e.g. CI Pipeline Token"
                      {...field}
                    />
                  </FormControl>
                  <FormDescription>
                    A descriptive name to identify this token.
                  </FormDescription>
                  <FormMessage />
                </FormItem>
              )}
            />

            <FormField
              control={form.control}
              name="role"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>Role</FormLabel>
                  <Select
                    value={field.value}
                    onValueChange={(value) => field.onChange(value)}
                    items={ROLE_LABELS}
                  >
                    <FormControl>
                      <SelectTrigger>
                        <SelectValue placeholder="Select role" />
                      </SelectTrigger>
                    </FormControl>
                    <SelectContent>
                      {availableRoles.map((role) => (
                        <SelectItem key={role} value={role}>
                          {ROLE_LABELS[role] ?? role}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                  <FormDescription>
                    The token will have the permissions of this role.
                  </FormDescription>
                  <FormMessage />
                </FormItem>
              )}
            />

            <FormField
              control={form.control}
              name="expiry"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>Expiry</FormLabel>
                  <Select
                    value={field.value}
                    onValueChange={(value) => field.onChange(value)}
                    items={EXPIRY_OPTIONS}
                  >
                    <FormControl>
                      <SelectTrigger>
                        <SelectValue placeholder="Select expiry" />
                      </SelectTrigger>
                    </FormControl>
                    <SelectContent>
                      {Object.entries(EXPIRY_OPTIONS).map(([key, label]) => (
                        <SelectItem key={key} value={key}>
                          {label}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                  <FormMessage />
                </FormItem>
              )}
            />

            <DialogFooter>
              <Button
                type="button"
                variant="outline"
                onClick={() => handleClose(false)}
              >
                Cancel
              </Button>
              <Button type="submit" disabled={createMutation.isPending}>
                {createMutation.isPending ? (
                  <>
                    <Spinner className="size-4" />
                    Creating...
                  </>
                ) : (
                  'Create Token'
                )}
              </Button>
            </DialogFooter>
          </form>
        </Form>
      </DialogContent>
    </Dialog>
  )
}

// ── Token Created Dialog ────────────────────────────────────────

interface TokenCreatedDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  response: CreateApiTokenResponse | null
}

function TokenCreatedDialog({
  open,
  onOpenChange,
  response,
}: TokenCreatedDialogProps) {
  const [copied, setCopied] = useState(false)

  function handleCopy() {
    if (!response) return
    void navigator.clipboard.writeText(response.token).then(() => {
      setCopied(true)
      toast.success('Token copied to clipboard')
      setTimeout(() => setCopied(false), 2000)
    })
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Token Created</DialogTitle>
          <DialogDescription>
            Make sure to copy your token now. You won&apos;t be able to see it
            again.
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-3">
          <div className="flex items-center gap-2">
            <code className="flex-1 break-all rounded-md bg-muted px-3 py-2 font-mono text-sm">
              {response?.token}
            </code>
            <Button variant="outline" size="sm" onClick={handleCopy}>
              {copied ? 'Copied' : 'Copy'}
            </Button>
          </div>

          <Alert>
            <AlertDescription>
              This token will not be shown again. Store it in a secure location.
            </AlertDescription>
          </Alert>
        </div>

        <DialogFooter>
          <Button onClick={() => onOpenChange(false)}>Done</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

// ── Main Page ───────────────────────────────────────────────────

function ApiTokensPage() {
  const { data, isLoading, error } = useApiTokens()
  const revokeMutation = useRevokeApiToken()
  const canWrite = useHasPermission('api_tokens', 'write')
  const canDelete = useHasPermission('api_tokens', 'delete')

  const [createOpen, setCreateOpen] = useState(false)
  const [createdResponse, setCreatedResponse] =
    useState<CreateApiTokenResponse | null>(null)
  const [createdDialogOpen, setCreatedDialogOpen] = useState(false)
  const [revokeTarget, setRevokeTarget] = useState<ApiTokenSummary | null>(null)

  const tokens = data?.tokens ?? []
  const activeCount = tokens.filter(
    (t) => !t.is_revoked && !t.is_expired,
  ).length

  function handleTokenCreated(response: CreateApiTokenResponse) {
    setCreatedResponse(response)
    setCreatedDialogOpen(true)
    toast.success('API token created')
  }

  function handleRevoke() {
    if (!revokeTarget) return
    revokeMutation.mutate(revokeTarget.id, {
      onSuccess: () => {
        toast.success(`Token "${revokeTarget.name}" revoked`)
        setRevokeTarget(null)
      },
      onError: (err) => {
        toast.error(getApiErrorMessage(err, {}))
        setRevokeTarget(null)
      },
    })
  }

  return (
    <PageLayout width="wide">
      <PageMeta title="API Tokens" noindex />
      <PageHeader
        title="API Tokens"
        description="Create and manage API tokens for programmatic access to your CI instance."
        actions={
          canWrite ? (
            <Button onClick={() => setCreateOpen(true)}>Create Token</Button>
          ) : undefined
        }
      />

      <section className="grid gap-4 md:grid-cols-3">
        <Card>
          <CardContent>
            <p className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
              Total tokens
            </p>
            <p className="mt-3 text-2xl font-bold tracking-tight">
              {tokens.length}
            </p>
            <p className="mt-1 text-xs text-muted-foreground">
              Active, expired, and revoked
            </p>
          </CardContent>
        </Card>
        <Card>
          <CardContent>
            <div className="flex items-center justify-between">
              <p className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                Active tokens
              </p>
              {activeCount > 0 ? (
                <Badge variant="success">{activeCount}</Badge>
              ) : null}
            </div>
            <p className="mt-3 text-2xl font-bold tracking-tight">
              {activeCount}
            </p>
            <p className="mt-1 text-xs text-muted-foreground">
              Currently valid for API access
            </p>
          </CardContent>
        </Card>
        <Card>
          <CardContent>
            <p className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
              Revoked tokens
            </p>
            <p className="mt-3 text-2xl font-bold tracking-tight">
              {tokens.filter((t) => t.is_revoked).length}
            </p>
            <p className="mt-1 text-xs text-muted-foreground">
              No longer valid
            </p>
          </CardContent>
        </Card>
      </section>

      {isLoading ? (
        <Card>
          <CardContent className="space-y-3">
            <Skeleton className="h-10 w-full" />
            <Skeleton className="h-10 w-full" />
            <Skeleton className="h-10 w-full" />
          </CardContent>
        </Card>
      ) : null}

      {error ? (
        <Alert variant="destructive">
          <AlertDescription>
            Failed to load API tokens: {error.message}
          </AlertDescription>
        </Alert>
      ) : null}

      {!isLoading && !error ? (
        <Card>
          <CardHeader>
            <CardTitle className="text-sm font-medium uppercase tracking-wider text-muted-foreground">
              Token Inventory
            </CardTitle>
          </CardHeader>
          <CardContent>
            {tokens.length === 0 ? (
              <p className="py-6 text-sm text-muted-foreground">
                No API tokens yet. Create one to get started.
              </p>
            ) : (
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>Name</TableHead>
                    <TableHead>Prefix</TableHead>
                    <TableHead>Role</TableHead>
                    <TableHead>Created by</TableHead>
                    <TableHead>Created</TableHead>
                    <TableHead>Last used</TableHead>
                    <TableHead>Status</TableHead>
                    <TableHead className="text-right">Actions</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {tokens.map((token) => {
                    const status = getTokenStatus(token)
                    return (
                      <TableRow key={token.id}>
                        <TableCell>
                          <p className="font-medium">{token.name}</p>
                        </TableCell>
                        <TableCell>
                          <code className="font-mono text-xs text-muted-foreground">
                            {token.prefix}...
                          </code>
                        </TableCell>
                        <TableCell>
                          <Badge variant="secondary">
                            {ROLE_LABELS[token.role] ?? token.role}
                          </Badge>
                        </TableCell>
                        <TableCell className="text-sm text-muted-foreground">
                          {token.created_by_email}
                        </TableCell>
                        <TableCell className="text-muted-foreground">
                          {formatRelativeTime(token.created_at)}
                        </TableCell>
                        <TableCell className="text-muted-foreground">
                          {formatRelativeTime(token.last_used_at)}
                        </TableCell>
                        <TableCell>
                          <Badge variant={getStatusVariant(status)}>
                            {status}
                          </Badge>
                        </TableCell>
                        <TableCell className="text-right">
                          {status === 'active' && canDelete ? (
                            <Button
                              variant="destructive"
                              size="sm"
                              onClick={() => setRevokeTarget(token)}
                            >
                              Revoke
                            </Button>
                          ) : null}
                        </TableCell>
                      </TableRow>
                    )
                  })}
                </TableBody>
              </Table>
            )}
          </CardContent>
        </Card>
      ) : null}

      <CreateTokenDialog
        open={createOpen}
        onOpenChange={setCreateOpen}
        onCreated={handleTokenCreated}
      />

      <TokenCreatedDialog
        open={createdDialogOpen}
        onOpenChange={(open) => {
          setCreatedDialogOpen(open)
          if (!open) setCreatedResponse(null)
        }}
        response={createdResponse}
      />

      <ConfirmDialog
        open={revokeTarget !== null}
        onOpenChange={(open) => {
          if (!open) setRevokeTarget(null)
        }}
        title="Revoke API Token"
        description="Are you sure you want to revoke this token? Any applications using this token will lose access immediately."
        confirmLabel="Revoke"
        confirmVariant="destructive"
        isPending={revokeMutation.isPending}
        onConfirm={handleRevoke}
      />
    </PageLayout>
  )
}
