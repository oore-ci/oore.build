import { useState } from 'react'
import {
  createLazyFileRoute,
  useNavigate,
  useSearch,
} from '@tanstack/react-router'
import { useForm } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import * as z from 'zod'
import { toast } from '@/lib/toast'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  Add01Icon,
  InformationCircleIcon,
  Search01Icon,
} from '@hugeicons/core-free-icons'

import type {
  ApiTokenSummary,
  CreateApiTokenResponse,
} from '@oore/client/models'
import { getApiErrorMessage } from '@/lib/api-client/api-error'
import { useAuthStore } from '@/stores/auth-store'
import { useHasPermissions } from '@/hooks/use-permissions'
import { usePageClamp } from '@/hooks/use-page-clamp'
import {
  useApiTokens,
  useCreateApiToken,
  useRevokeApiToken,
} from '@/hooks/use-api-tokens'
import { PageMeta } from '@/lib/seo'
import PageLayout from '@/components/page-layout'
import PageHeader from '@/components/page-header'
import { Alert, AlertDescription } from '@/components/ui/alert'
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
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from '@/components/ui/empty'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Spinner } from '@/components/ui/spinner'
import ConfirmDialog from '@/components/ConfirmDialog'
import TokenCreatedDialog from '@/components/token-created-dialog'
import type { SortDirection } from '@/components/data-table-features'
import type { ApiTokenSort, ApiTokensSearch } from './api-tokens'
import { ApiTokenInventory } from './-api-token-inventory'
import { ROLE_DESCRIPTIONS, ROLE_LABELS } from './-user-role-labels'

export const Route = createLazyFileRoute('/settings/api-tokens')({
  component: ApiTokensPage,
})

const ROLE_HIERARCHY: Array<string> = [
  'owner',
  'admin',
  'developer',
  'qa_viewer',
]

const EXPIRY_OPTIONS = {
  never: 'Never',
  '30': '30 days',
  '90': '90 days',
  '365': '1 year',
} satisfies Record<string, string>

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
  // Developer is the least privileged role that can trigger builds, which is
  // what most tokens are for. Users below it can only mint their own role.
  const defaultRole = availableRoles.includes('developer')
    ? 'developer'
    : (authUser?.role ?? 'qa_viewer')

  const form = useForm<CreateTokenFormValues>({
    resolver: zodResolver(createTokenSchema),
    defaultValues: {
      name: '',
      role: defaultRole,
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
        : Math.floor(Date.now() / 1000) + Number(values.expiry) * 24 * 60 * 60

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
          <DialogTitle>Create API token</DialogTitle>
          <DialogDescription>
            Create a new API token for programmatic access to this instance.
          </DialogDescription>
        </DialogHeader>

        <Form {...form}>
          <form
            onSubmit={(event) => void form.handleSubmit(onSubmit)(event)}
            className="space-y-4"
          >
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
                      <SelectTrigger className="w-full">
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
                    {ROLE_DESCRIPTIONS[field.value] ??
                      'The token will have the permissions of this role.'}
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
                      <SelectTrigger className="w-full">
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
                  'Create token'
                )}
              </Button>
            </DialogFooter>
          </form>
        </Form>
      </DialogContent>
    </Dialog>
  )
}

function ApiTokensPage() {
  const navigate = useNavigate({ from: '/settings/api-tokens' })
  const search = useSearch({ from: '/settings/api-tokens' })
  const revokeMutation = useRevokeApiToken()
  const [canWrite, canDelete] = useHasPermissions([
    'api_tokens:write',
    'api_tokens:delete',
  ])

  const [createOpen, setCreateOpen] = useState(false)
  const [createdResponse, setCreatedResponse] =
    useState<CreateApiTokenResponse | null>(null)
  const [revokeTarget, setRevokeTarget] = useState<ApiTokenSummary | null>(null)

  const page = search.page ?? 1
  const pageSize = search.pageSize ?? 20
  const sort = search.sort ?? 'created_at'
  const direction = search.direction ?? 'desc'
  const tokensQuery = useApiTokens({
    q: search.q,
    sort,
    direction,
    limit: pageSize,
    offset: (page - 1) * pageSize,
  })
  const total = tokensQuery.data?.total ?? 0
  const currentPage = Math.min(page, Math.max(1, Math.ceil(total / pageSize)))
  const visibleTokens = tokensQuery.data?.tokens ?? []
  function updateSearch(updates: Partial<ApiTokensSearch>) {
    void navigate({
      search: (previous) => ({ ...previous, ...updates }),
      replace: true,
    })
  }

  usePageClamp(
    page,
    pageSize,
    tokensQuery.isLoading ? undefined : total,
    (nextPage) => {
      updateSearch({ page: nextPage === 1 ? undefined : nextPage })
    },
  )

  function handleSortChange(nextSort: ApiTokenSort, next: SortDirection) {
    updateSearch({ sort: nextSort, direction: next, page: undefined })
  }

  function handleTokenCreated(response: CreateApiTokenResponse) {
    setCreatedResponse(response)
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
    <PageLayout width="wide" fill>
      <PageMeta title="API tokens" noindex />
      <PageHeader
        title="API tokens"
        description="Manage credentials for programmatic access to this instance."
        actions={
          canWrite ? (
            <Button onClick={() => setCreateOpen(true)}>
              <HugeiconsIcon icon={Add01Icon} />
              Create token
            </Button>
          ) : undefined
        }
      />

      {tokensQuery.error ? (
        <Alert variant="destructive">
          <HugeiconsIcon icon={InformationCircleIcon} />
          <AlertDescription className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
            <span>Failed to load API tokens: {tokensQuery.error.message}</span>
            <Button
              variant="outline"
              size="sm"
              onClick={() => void tokensQuery.refetch()}
            >
              Retry
            </Button>
          </AlertDescription>
        </Alert>
      ) : null}

      {!tokensQuery.isLoading &&
      !tokensQuery.error &&
      visibleTokens.length === 0 &&
      !search.q ? (
        <Empty className="border bg-card">
          <EmptyHeader>
            <EmptyTitle>No API tokens yet</EmptyTitle>
            <EmptyDescription>
              Create a token when an integration needs programmatic access.
            </EmptyDescription>
          </EmptyHeader>
          {canWrite ? (
            <EmptyContent>
              <Button onClick={() => setCreateOpen(true)}>Create token</Button>
            </EmptyContent>
          ) : null}
        </Empty>
      ) : null}

      {!tokensQuery.isLoading &&
      !tokensQuery.error &&
      visibleTokens.length === 0 &&
      search.q ? (
        <Empty className="border bg-card">
          <EmptyHeader>
            <EmptyMedia variant="icon">
              <HugeiconsIcon icon={Search01Icon} />
            </EmptyMedia>
            <EmptyTitle>No matching tokens</EmptyTitle>
            <EmptyDescription>
              Try a different search or clear the current query.
            </EmptyDescription>
          </EmptyHeader>
          <EmptyContent>
            <Button
              variant="outline"
              onClick={() => updateSearch({ q: undefined, page: undefined })}
            >
              Clear search
            </Button>
          </EmptyContent>
        </Empty>
      ) : null}

      {!tokensQuery.error && (tokensQuery.isLoading || total > 0) ? (
        <ApiTokenInventory
          canDelete={canDelete}
          direction={direction}
          isLoading={tokensQuery.isLoading}
          onPageChange={(nextPage) =>
            updateSearch({ page: nextPage > 1 ? nextPage : undefined })
          }
          onRevoke={setRevokeTarget}
          onSearch={(value) =>
            updateSearch({ q: value.trim() || undefined, page: undefined })
          }
          onSortChange={handleSortChange}
          page={currentPage}
          pageSize={pageSize}
          query={search.q ?? ''}
          sort={sort}
          tokens={visibleTokens}
          total={total}
        />
      ) : null}

      <CreateTokenDialog
        open={createOpen}
        onOpenChange={setCreateOpen}
        onCreated={handleTokenCreated}
      />

      <TokenCreatedDialog
        open={createdResponse !== null}
        onOpenChange={(open) => {
          if (!open) setCreatedResponse(null)
        }}
        response={createdResponse}
      />

      <ConfirmDialog
        open={revokeTarget !== null}
        onOpenChange={(open) => {
          if (!open) setRevokeTarget(null)
        }}
        title="Revoke API token"
        description="Are you sure you want to revoke this token? Any applications using this token will lose access immediately."
        confirmLabel="Revoke"
        confirmVariant="destructive"
        isPending={revokeMutation.isPending}
        onConfirm={handleRevoke}
      />
    </PageLayout>
  )
}
