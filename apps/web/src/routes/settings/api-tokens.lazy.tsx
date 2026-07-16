import { useMemo, useState } from 'react'
import {
  createLazyFileRoute,
  useNavigate,
  useSearch,
} from '@tanstack/react-router'
import { useForm } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import * as z from 'zod'
import { toast } from 'sonner'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  Add01Icon,
  InformationCircleIcon,
  Search01Icon,
} from '@hugeicons/core-free-icons'

import type { ApiTokenSummary, CreateApiTokenResponse } from '@/lib/types'
import { getApiErrorMessage } from '@/lib/api'
import { useAuthStore } from '@/stores/auth-store'
import { useHasPermission } from '@/hooks/use-permissions'
import { useDebouncedCallback } from '@/hooks/use-debounced-callback'
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
import { Card, CardContent } from '@/components/ui/card'
import ConfirmDialog from '@/components/ConfirmDialog'
import TokenCreatedDialog from '@/components/token-created-dialog'
import {
  CollectionPagination,
  SortableTableHead,
} from '@/components/collection-controls'
import type { SortDirection } from '@/components/collection-controls'
import type { ApiTokenSort, ApiTokensSearch } from './api-tokens'

export const Route = createLazyFileRoute('/settings/api-tokens')({
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
): 'secondary' | 'outline' | 'destructive' {
  switch (status) {
    case 'active':
      return 'secondary'
    case 'expired':
      return 'outline'
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

// ── Main Page ───────────────────────────────────────────────────

const API_TOKEN_SORT_OPTIONS: Record<ApiTokenSort, string> = {
  created_at: 'Created',
  last_used_at: 'Last used',
  name: 'Name',
  role: 'Role',
  status: 'Status',
}

function TokenSearch({
  initialValue,
  onSearch,
}: {
  initialValue: string
  onSearch: (value: string) => void
}) {
  const [value, setValue] = useState(initialValue)
  const debouncedSearch = useDebouncedCallback(onSearch, 300)

  return (
    <div className="relative w-full sm:max-w-sm">
      <HugeiconsIcon
        icon={Search01Icon}
        className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground"
        aria-hidden
      />
      <Input
        type="search"
        value={value}
        onChange={(event) => {
          const next = event.target.value
          setValue(next)
          debouncedSearch(next)
        }}
        placeholder="Search API tokens"
        aria-label="Search API tokens"
        className="pl-9"
      />
    </div>
  )
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
  const [createdDialogOpen, setCreatedDialogOpen] = useState(false)
  const [revokeTarget, setRevokeTarget] = useState<ApiTokenSummary | null>(null)

  const page = search.page ?? 1
  const pageSize = search.pageSize ?? 20
  const sort = search.sort ?? 'created_at'
  const direction = search.direction ?? 'desc'
  const tokens = tokensQuery.data?.tokens ?? []
  const activeCount = tokens.filter(
    (t) => !t.is_revoked && !t.is_expired,
  ).length
  const filteredTokens = useMemo(() => {
    const query = search.q?.toLowerCase()
    if (!query) return tokens

    return tokens.filter((token) =>
      [
        token.name,
        token.prefix,
        token.role,
        token.created_by_email,
        getTokenStatus(token),
      ].some((value) => value.toLowerCase().includes(query)),
    )
  }, [search.q, tokens])
  const sortedTokens = useMemo(
    () =>
      [...filteredTokens].sort((left, right) => {
        const result = compareTokens(left, right, sort)
        return direction === 'asc' ? result : -result
      }),
    [direction, filteredTokens, sort],
  )
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
    setCreatedResponse(() => response)
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
            <Button onClick={() => setCreateOpen(true)}>
              <HugeiconsIcon icon={Add01Icon} />
              Create token
            </Button>
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
                <Badge variant="secondary">{activeCount}</Badge>
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

      <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <TokenSearch
          key={search.q ?? ''}
          initialValue={search.q ?? ''}
          onSearch={(value) =>
            updateSearch({ q: value.trim() || undefined, page: undefined })
          }
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
          <HugeiconsIcon icon={InformationCircleIcon} size={16} />
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
        <Empty className="bg-card">
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
        <Empty className="bg-card">
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
        <section aria-label="API token inventory" className="min-w-0">
          <div className="divide-y sm:hidden">
            {tokensQuery.isLoading
              ? Array.from({ length: 4 }, (_, index) => (
                  <div key={index} className="space-y-2 py-4">
                    <Skeleton className="h-5 w-2/3" />
                    <Skeleton className="h-4 w-1/2" />
                  </div>
                ))
              : visibleTokens.map((token) => {
                  const status = getTokenStatus(token)
                  return (
                    <article key={token.id} className="space-y-3 py-4">
                      <div className="flex items-start justify-between gap-3">
                        <div className="min-w-0">
                          <h2 className="truncate font-medium">{token.name}</h2>
                          <code className="block truncate font-mono text-xs text-muted-foreground">
                            {token.prefix}...
                          </code>
                        </div>
                        <Badge variant={getStatusVariant(status)}>
                          {status}
                        </Badge>
                      </div>
                      <div className="flex flex-wrap items-center gap-x-3 gap-y-2 text-xs text-muted-foreground">
                        <Badge variant="secondary">
                          {ROLE_LABELS[token.role] ?? token.role}
                        </Badge>
                        <span>
                          Created {formatRelativeTime(token.created_at)}
                        </span>
                        <span>
                          Used {formatRelativeTime(token.last_used_at)}
                        </span>
                      </div>
                      {status === 'active' && canDelete ? (
                        <Button
                          variant="destructive"
                          size="sm"
                          onClick={() => setRevokeTarget(token)}
                        >
                          Revoke
                        </Button>
                      ) : null}
                    </article>
                  )
                })}
          </div>

          <div className="hidden sm:block">
            <Table>
              <TableHeader>
                <TableRow>
                  <SortableTableHead
                    sort={sort}
                    sortKey="name"
                    direction={direction}
                    onSortChange={handleSortChange}
                  >
                    Name
                  </SortableTableHead>
                  <TableHead className="hidden lg:table-cell">Prefix</TableHead>
                  <SortableTableHead
                    sort={sort}
                    sortKey="role"
                    direction={direction}
                    onSortChange={handleSortChange}
                  >
                    Role
                  </SortableTableHead>
                  <TableHead className="hidden lg:table-cell">
                    Created by
                  </TableHead>
                  <SortableTableHead
                    className="hidden lg:table-cell"
                    sort={sort}
                    sortKey="created_at"
                    direction={direction}
                    onSortChange={handleSortChange}
                  >
                    Created
                  </SortableTableHead>
                  <SortableTableHead
                    className="hidden lg:table-cell"
                    sort={sort}
                    sortKey="last_used_at"
                    direction={direction}
                    onSortChange={handleSortChange}
                  >
                    Last used
                  </SortableTableHead>
                  <SortableTableHead
                    sort={sort}
                    sortKey="status"
                    direction={direction}
                    onSortChange={handleSortChange}
                  >
                    Status
                  </SortableTableHead>
                  <TableHead className="text-right">Actions</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {tokensQuery.isLoading
                  ? Array.from({ length: 5 }, (_, index) => (
                      <TableRow key={index}>
                        <TableCell>
                          <Skeleton className="h-5 w-36" />
                        </TableCell>
                        <TableCell className="hidden lg:table-cell">
                          <Skeleton className="h-4 w-20" />
                        </TableCell>
                        <TableCell>
                          <Skeleton className="h-6 w-20" />
                        </TableCell>
                        <TableCell className="hidden lg:table-cell">
                          <Skeleton className="h-4 w-40" />
                        </TableCell>
                        <TableCell className="hidden lg:table-cell">
                          <Skeleton className="h-4 w-20" />
                        </TableCell>
                        <TableCell className="hidden lg:table-cell">
                          <Skeleton className="h-4 w-20" />
                        </TableCell>
                        <TableCell>
                          <Skeleton className="h-6 w-16" />
                        </TableCell>
                        <TableCell>
                          <Skeleton className="ml-auto h-8 w-16" />
                        </TableCell>
                      </TableRow>
                    ))
                  : visibleTokens.map((token) => {
                      const status = getTokenStatus(token)
                      return (
                        <TableRow key={token.id}>
                          <TableCell className="font-medium">
                            {token.name}
                          </TableCell>
                          <TableCell className="hidden lg:table-cell">
                            <code className="font-mono text-xs text-muted-foreground">
                              {token.prefix}...
                            </code>
                          </TableCell>
                          <TableCell>
                            <Badge variant="secondary">
                              {ROLE_LABELS[token.role] ?? token.role}
                            </Badge>
                          </TableCell>
                          <TableCell className="hidden text-sm text-muted-foreground lg:table-cell">
                            {token.created_by_email}
                          </TableCell>
                          <TableCell className="hidden text-muted-foreground lg:table-cell">
                            {formatRelativeTime(token.created_at)}
                          </TableCell>
                          <TableCell className="hidden text-muted-foreground lg:table-cell">
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
          </div>

          {!tokensQuery.isLoading ? (
            <CollectionPagination
              page={currentPage}
              pageSize={pageSize}
              total={total}
              onPageChange={(nextPage) =>
                updateSearch({ page: nextPage > 1 ? nextPage : undefined })
              }
              onPageSizeChange={(nextPageSize) =>
                updateSearch({
                  pageSize:
                    nextPageSize === 20
                      ? undefined
                      : (nextPageSize as 50 | 100),
                  page: undefined,
                })
              }
            />
          ) : null}
        </section>
      ) : null}

      <CreateTokenDialog
        open={createOpen}
        onOpenChange={setCreateOpen}
        onCreated={handleTokenCreated}
      />

      <TokenCreatedDialog
        open={createdDialogOpen}
        onOpenChange={(open) => {
          setCreatedDialogOpen(() => open)
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
