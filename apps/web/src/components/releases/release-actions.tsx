import { useState } from "react";
import { HugeiconsIcon } from "@hugeicons/react";
import {
  MoreHorizontalCircle01Icon,
  StopCircleIcon,
} from "@hugeicons/core-free-icons";

import type { Release, ReleaseAction } from "@oore/client/models";
import { Button } from "@/components/ui/button";
import {
  AlertDialog,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
  InputGroup,
  InputGroupAddon,
  InputGroupInput,
  InputGroupText,
} from "@/components/ui/input-group";
import { Label } from "@/components/ui/label";
import { Spinner } from "@/components/ui/spinner";
import { Textarea } from "@/components/ui/textarea";
import { useReleaseAction } from "@/hooks/use-releases";
import {
  RELEASE_ACTION_COPY,
  RELEASE_ACTION_ORDER,
  RELEASE_STORE_LABEL,
  fractionToPercent,
  releaseAppName,
  releaseVersionLabel,
  rolloutPercentError,
} from "@/lib/releases";
import { toast } from "@/lib/toast";
import { Row } from "@/components/layout/row";
import { Stack } from "@/components/layout/stack";

/**
 * Release controls. The server decides which actions the caller may take
 * from their project role and the release state; a viewer gets none.
 * Halt sits outside the menu so it is always two clicks away.
 */
export function ReleaseActions({
  release,
  size = "sm",
}: {
  release: Release;
  size?: "sm" | "default";
}) {
  const [pending, setPending] = useState<ReleaseAction | null>(null);
  const allowed = new Set(release.allowed_actions);
  const canHalt = allowed.has("halt") && release.state !== "halted";
  const menuActions = RELEASE_ACTION_ORDER.filter(
    (action) => action !== "halt" && allowed.has(action),
  );

  if (!canHalt && menuActions.length === 0) return null;

  return (
    <Row gap={1} className="justify-end">
      {canHalt ? (
        <Button
          variant="outline"
          size={size}
          onClick={() => setPending("halt")}
          aria-label={`Halt rollout of ${releaseVersionLabel(release)}`}
        >
          <HugeiconsIcon icon={StopCircleIcon} data-icon="inline-start" />
          Halt
        </Button>
      ) : null}
      {menuActions.length > 0 ? (
        <DropdownMenu>
          <DropdownMenuTrigger
            render={
              <Button
                variant="ghost"
                size={size === "sm" ? "icon-sm" : "icon"}
              />
            }
          >
            <span className="sr-only">
              Release actions for {releaseVersionLabel(release)}
            </span>
            <HugeiconsIcon icon={MoreHorizontalCircle01Icon} />
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end" className="w-44">
            <DropdownMenuGroup>
              <DropdownMenuLabel>Actions</DropdownMenuLabel>
              {menuActions.map((action) => (
                <DropdownMenuItem
                  key={action}
                  onClick={() => setPending(action)}
                >
                  {RELEASE_ACTION_COPY[action].label}
                </DropdownMenuItem>
              ))}
            </DropdownMenuGroup>
          </DropdownMenuContent>
        </DropdownMenu>
      ) : null}
      {pending ? (
        <ReleaseActionDialog
          action={pending}
          release={release}
          onOpenChange={(open) => {
            if (!open) setPending(null);
          }}
        />
      ) : null}
    </Row>
  );
}

function ReleaseActionDialog({
  action,
  onOpenChange,
  release,
}: {
  action: ReleaseAction;
  onOpenChange: (open: boolean) => void;
  release: Release;
}) {
  const [reason, setReason] = useState("");
  // Raise takes an exact target; start from the next policy step.
  const [percent, setPercent] = useState(() =>
    String(fractionToPercent(release.next_fraction ?? 1)),
  );
  const [showPercentError, setShowPercentError] = useState(false);
  const mutation = useReleaseAction();
  const copy = RELEASE_ACTION_COPY[action];
  const reasonId = `release-${release.id}-${action}-reason`;
  const percentId = `release-${release.id}-percent`;
  const percentError =
    action === "raise"
      ? rolloutPercentError(percent, release.rollout_fraction)
      : null;

  function confirm() {
    if (percentError) {
      setShowPercentError(true);
      return;
    }
    const fraction =
      action === "raise" ? Math.round(Number(percent) * 100) / 10_000 : null;
    // The promise settles even if the dialog is gone, unlike mutate callbacks.
    mutation
      .mutateAsync({
        action,
        releaseId: release.id,
        body: { reason: reason.trim() || null, fraction },
      })
      .then(
        (updated) => {
          toast.success(
            action === "raise"
              ? `${releaseVersionLabel(release)} rollout set to ${fractionToPercent(updated.rollout_fraction)}%`
              : `${copy.done}: ${releaseVersionLabel(release)}`,
          );
          onOpenChange(false);
        },
        (error: Error) => {
          toast.error(
            `Could not ${copy.label.toLowerCase()}: ${error.message}`,
          );
        },
      );
  }

  return (
    <AlertDialog
      open
      onOpenChange={(open) => {
        if (!open && mutation.isPending) return;
        onOpenChange(open);
      }}
    >
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{copy.title}</AlertDialogTitle>
          <p className="text-sm font-medium">
            {releaseAppName(release)} · {RELEASE_STORE_LABEL[release.store]}{" "}
            {release.track} · {releaseVersionLabel(release)}
          </p>
          <AlertDialogDescription>
            {copy.description(release)}
          </AlertDialogDescription>
        </AlertDialogHeader>
        {action === "raise" ? (
          <Stack gap={2}>
            <Label htmlFor={percentId}>New rollout</Label>
            <InputGroup className="max-w-40">
              <InputGroupInput
                id={percentId}
                type="number"
                inputMode="decimal"
                step="any"
                min={0}
                max={100}
                value={percent}
                onChange={(event) => setPercent(event.target.value)}
                onBlur={() => setShowPercentError(true)}
                onKeyDown={(event) => {
                  if (event.key === "Enter") confirm();
                }}
                aria-invalid={
                  showPercentError && percentError ? true : undefined
                }
                aria-describedby={`${percentId}-hint`}
                autoFocus
              />
              <InputGroupAddon align="inline-end">
                <InputGroupText>%</InputGroupText>
              </InputGroupAddon>
            </InputGroup>
            {showPercentError && percentError ? (
              <p id={`${percentId}-hint`} className="text-sm text-destructive">
                {percentError}
              </p>
            ) : (
              <p
                id={`${percentId}-hint`}
                className="text-sm text-muted-foreground"
              >
                {Number(percent) >= 100
                  ? "At 100% the release is complete."
                  : "Later schedule steps above this still apply."}
              </p>
            )}
          </Stack>
        ) : null}
        <Stack gap={2}>
          <Label htmlFor={reasonId}>Reason (optional)</Label>
          <Textarea
            id={reasonId}
            value={reason}
            onChange={(event) => setReason(event.target.value)}
            placeholder="Recorded in the release history"
            rows={2}
          />
        </Stack>
        <AlertDialogFooter>
          <Button
            variant="outline"
            onClick={() => onOpenChange(false)}
            disabled={mutation.isPending}
          >
            Cancel
          </Button>
          <Button
            variant={"destructive" in copy ? "destructive" : "default"}
            onClick={confirm}
            disabled={mutation.isPending}
            autoFocus={action !== "raise"}
          >
            {mutation.isPending ? (
              <span className="flex items-center gap-1.5" role="status">
                <Spinner data-icon="inline-start" />
                Please wait...
              </span>
            ) : (
              copy.confirmLabel
            )}
          </Button>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
