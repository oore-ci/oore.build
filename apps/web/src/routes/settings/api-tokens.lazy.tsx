import { useMemo, useState } from 'react'
import {
  createLazyFileRoute,
  useNavigate,
  useSearch,
} from '@tanstack/react-router'
import { useForm } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import * as z from 'zod'
import { toast } from '@/lib/toast'
import {
  Plus as Add01Icon,
  Info as InformationCircleIcon,
  Search as Search01Icon,
} from 'lucide-react'

import type { ApiTokenSummary, CreateApiTokenResponse } from '@/lib/types'
import { getApiErrorMessage } from '@/lib/api'
import { useAuthStore } from '@/stores/auth-store'
import { useHasPermission } from '@/hooks/use-permissions'
import { CollectionSearchInput } from '@/components/collection-search-input'
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
import { NativeSelect, NativeSelectOption } from '@/components/ui/native-select'
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
import type { SortDirection } from '@/components/collection-controls'
import type { ApiTokenSort, ApiTokensSearch } from './api-tokens'
import { ApiTokenInventory } from './-api-token-inventory'
import { ApiTokenStats } from './-api-token-summary'
import { ROLE_LABELS } from './-user-role-labels'

export const Route = createLazyFileRoute('/settings/api-tokens')({
  component: ApiTokensPage,
})

const EMPTY_API_TOKENS: Array<ApiTokenSummary> = []

function getTokenStatus(
  token: ApiTokenSummary,
): 'active' | 'expired' | 'revoked' {
  if (token.is_revoked) return 'revoked'
  if (token.is_expired) return 'expired'
  return 'active'
}

const ROLE_HIERARCHY: Array<string> = [
  'owner',
  'admin',
  'developer',
  'qa_viewer',
]

const EXPIRY_OPTIONS: Record<string, string> = {
  never: 'Never',
  '30': '30 days',
  '90': '90 days',
  '365': '1 year',
}

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

const API_TOKEN_SORT_OPTIONS: Record<ApiTokenSort, string> = {
  created_at: 'Created',
  last_used_at: 'Last used',
  name: 'Name',
  role: 'Role',
  status: 'Status',
}

function compareTokens(
  left: ApiTokenSummary,
  right: ApiTokenSummary,
  sort: ApiTokenSort,
): number {
  let result = 0

  switch (sort) {
    case 'name':
      result = left.name.localeCompare(right.name)
      break
    case 'role':
      result = left.role.localeCompare(right.role)
      break
    case 'status':
      result = getTokenStatus(left).localeCompare(getTokenStatus(right))
      break
    case 'last_used_at':
      result = (left.last_used_at ?? 0) - (right.last_used_at ?? 0)
      break
    case 'created_at':
      result = left.created_at - right.created_at
      break
  }

  return result || left.id.localeCompare(right.id)
}

function ApiTokensPage() {
  const tokensQuery = useApiTokens()
  const navigate = useNavigate({ from: '/settings/api-tokens' })
  const search = useSearch({ from: '/settings/api-tokens' })
  const revokeMutation = useRevokeApiToken()
  const canWrite = useHasPermission('api_tokens', 'write')
  const canDelete = useHasPermission('api_tokens', 'delete')

  const [createOpen, setCreateOpen] = useState(false)
  const [createdResponse, setCreatedResponse] =
    useState<CreateApiTokenResponse | null>(null)
  const [revokeTarget, setRevokeTarget] = useState<ApiTokenSummary | null>(null)

  const page = search.page ?? 1
  const pageSize = search.pageSize ?? 20
  const sort = search.sort ?? 'created_at'
  const direction = search.direction ?? 'desc'
  const tokens = tokensQuery.data?.tokens ?? EMPTY_API_TOKENS
  const activeCount = tokens.filter(
    (token) => !token.is_revoked && !token.is_expired,
  ).length
  const sortedTokens = useMemo(() => {
    const query = search.q?.toLowerCase()
    const matchingTokens = query
      ? tokens.filter((token) =>
          [
            token.name,
            token.prefix,
            token.role,
            token.created_by_email,
            getTokenStatus(token),
          ].some((value) => value.toLowerCase().includes(query)),
        )
      : tokens

    return [...matchingTokens].sort((left, right) => {
      const result = compareTokens(left, right, sort)
      return direction === 'asc' ? result : -result
    })
  }, [direction, search.q, sort, tokens])
  const total = sortedTokens.length
  const currentPage = Math.min(page, Math.max(1, Math.ceil(total / pageSize)))
  const visibleTokens = sortedTokens.slice(
    (currentPage - 1) * pageSize,
    currentPage * pageSize,
  )
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
    <PageLayout width="wide">
      <PageMeta title="API tokens" noindex />
      <PageHeader
        title="API tokens"
        description="Create and manage API tokens for programmatic access to your CI instance."
        actions={
          canWrite ? (
            <Button onClick={() => setCreateOpen(true)}>
              <Add01Icon />
              Create token
            </Button>
          ) : undefined
        }
      />

      {!tokensQuery.error ? (
        <ApiTokenStats
          active={activeCount}
          isLoading={tokensQuery.isLoading}
          revoked={tokens.filter((token) => token.is_revoked).length}
          total={tokens.length}
        />
      ) : null}

      <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <CollectionSearchInput
          initialValue={search.q ?? ''}
          onSearch={(value) =>
            updateSearch({ q: value.trim() || undefined, page: undefined })
          }
          placeholder="Search API tokens"
          ariaLabel="Search API tokens"
        />
        <div className="flex gap-2 sm:hidden">
          <NativeSelect
            className="min-w-0 flex-1"
            aria-label="Sort API tokens"
            value={sort}
            onChange={(event) =>
              handleSortChange(event.target.value as ApiTokenSort, direction)
            }
          >
            {Object.entries(API_TOKEN_SORT_OPTIONS).map(([value, label]) => (
              <NativeSelectOption key={value} value={value}>
                {label}
              </NativeSelectOption>
            ))}
          </NativeSelect>
          <Button
            variant="outline"
            onClick={() =>
              handleSortChange(sort, direction === 'asc' ? 'desc' : 'asc')
            }
          >
            {direction === 'asc' ? 'Ascending' : 'Descending'}
          </Button>
        </div>
      </div>

      {tokensQuery.error ? (
        <Alert variant="destructive">
          <InformationCircleIcon size={16} />
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

      {!tokensQuery.isLoading && !tokensQuery.error && tokens.length === 0 ? (
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
      tokens.length > 0 &&
      total === 0 ? (
        <Empty className="border bg-card">
          <EmptyHeader>
            <EmptyMedia variant="icon">
              <Search01Icon />
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
          onPageSizeChange={(nextPageSize) =>
            updateSearch({
              pageSize:
                nextPageSize === 20 ? undefined : (nextPageSize as 50 | 100),
              page: undefined,
            })
          }
          onRevoke={setRevokeTarget}
          onSortChange={handleSortChange}
          page={currentPage}
          pageSize={pageSize}
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
