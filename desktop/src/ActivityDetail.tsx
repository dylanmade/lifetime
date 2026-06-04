import { useState } from "react";
import { Trash2 } from "lucide-react";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Label } from "@/components/ui/label";
import { Badge } from "@/components/ui/badge";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { deleteActivity, updateActivity, type ResolvedActivity } from "./api";
import {
  formatDuration,
  formatTimeRange,
  toDateTimeLocalValue,
} from "@/lib/datetime";

type Props = {
  // The activity to show; `null` keeps the dialog closed.
  activity: ResolvedActivity | null;
  onOpenChange: (open: boolean) => void;
  // Called after a successful save or delete so the parent can refetch.
  onChanged: () => void;
};

export function ActivityDetail({ activity, onOpenChange, onChanged }: Props) {
  return (
    <Dialog open={activity !== null} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg">
        {/* Keyed by id so editing state resets when a different activity opens. */}
        {activity && (
          <Body
            key={activity.id}
            activity={activity}
            onClose={() => onOpenChange(false)}
            onChanged={onChanged}
          />
        )}
      </DialogContent>
    </Dialog>
  );
}

function Body({
  activity,
  onClose,
  onChanged,
}: {
  activity: ResolvedActivity;
  onClose: () => void;
  onChanged: () => void;
}) {
  const isManual = activity.source === "manual";

  const [title, setTitle] = useState(activity.title);
  const [category, setCategory] = useState(activity.category ?? "");
  const [description, setDescription] = useState(activity.description ?? "");
  const [startLocal, setStartLocal] = useState(() =>
    toDateTimeLocalValue(new Date(activity.starts_at)),
  );
  const [endLocal, setEndLocal] = useState(() =>
    activity.ends_at ? toDateTimeLocalValue(new Date(activity.ends_at)) : "",
  );
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [confirmingDelete, setConfirmingDelete] = useState(false);

  const durationSeconds = activity.ends_at
    ? (new Date(activity.ends_at).getTime() -
        new Date(activity.starts_at).getTime()) /
      1000
    : 0;

  async function save() {
    setError(null);
    const fields: Parameters<typeof updateActivity>[1] = {
      category,
      description,
    };

    if (isManual) {
      const trimmed = title.trim();
      if (!trimmed) return setError("Title is required.");
      if (!startLocal) return setError("Start time is required.");
      const start = new Date(startLocal);
      if (Number.isNaN(start.getTime())) return setError("Invalid start time.");
      let end: Date | null = null;
      if (endLocal) {
        end = new Date(endLocal);
        if (Number.isNaN(end.getTime())) return setError("Invalid end time.");
        if (end <= start) return setError("End time must be after start time.");
      }
      fields.title = trimmed;
      fields.startsAtIso = start.toISOString();
      fields.endsAtIso = end ? end.toISOString() : ""; // "" ⇒ open-ended
    }

    setBusy(true);
    try {
      await updateActivity(activity.id, fields);
      onChanged();
      onClose();
    } catch (e) {
      setError(String(e));
      setBusy(false);
    }
  }

  async function remove() {
    setBusy(true);
    try {
      await deleteActivity(activity.id);
      onChanged();
      onClose();
    } catch (e) {
      setError(String(e));
      setBusy(false);
    }
  }

  return (
    <>
      <DialogHeader>
        <div className="flex items-center gap-2">
          <DialogTitle>Activity details</DialogTitle>
          <Badge variant={isManual ? "default" : "secondary"}>
            {isManual ? "Manual" : "Auto-tracked"}
          </Badge>
        </div>
      </DialogHeader>

      <form
        onSubmit={(e) => {
          e.preventDefault();
          save();
        }}
        className="space-y-4"
      >
        {isManual ? (
          <>
            <div className="space-y-1.5">
              <Label htmlFor="ad-title">Title</Label>
              <Input
                id="ad-title"
                value={title}
                onChange={(e) => setTitle(e.target.value)}
                autoFocus
                maxLength={200}
              />
            </div>
            <div className="grid grid-cols-2 gap-3">
              <div className="space-y-1.5">
                <Label htmlFor="ad-start">Start</Label>
                <Input
                  id="ad-start"
                  type="datetime-local"
                  value={startLocal}
                  onChange={(e) => setStartLocal(e.target.value)}
                />
              </div>
              <div className="space-y-1.5">
                <Label htmlFor="ad-end" className="flex items-baseline gap-1.5">
                  End{" "}
                  <span className="text-muted-foreground text-xs font-normal">
                    optional
                  </span>
                </Label>
                <Input
                  id="ad-end"
                  type="datetime-local"
                  value={endLocal}
                  onChange={(e) => setEndLocal(e.target.value)}
                />
              </div>
            </div>
          </>
        ) : (
          <div className="bg-muted/40 space-y-1 rounded-lg p-3">
            <p className="font-medium">{activity.app_name}</p>
            {activity.window_title && (
              <p className="text-muted-foreground text-sm">
                {activity.window_title}
              </p>
            )}
            <p className="text-muted-foreground text-sm tabular-nums">
              {formatTimeRange(activity.starts_at, activity.ends_at)} ·{" "}
              {formatDuration(durationSeconds)}
              {activity.is_active === false && " · idle"}
            </p>
          </div>
        )}

        <div className="space-y-1.5">
          <Label htmlFor="ad-category">Category</Label>
          <Input
            id="ad-category"
            value={category}
            onChange={(e) => setCategory(e.target.value)}
            placeholder="Unassigned"
            autoFocus={!isManual}
            maxLength={100}
          />
        </div>
        <div className="space-y-1.5">
          <Label htmlFor="ad-note">{isManual ? "Notes" : "Note"}</Label>
          <Textarea
            id="ad-note"
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            rows={2}
            maxLength={1000}
            placeholder="Optional"
          />
        </div>

        {error && (
          <Alert variant="destructive">
            <AlertDescription>{error}</AlertDescription>
          </Alert>
        )}

        <DialogFooter className="sm:justify-between">
          {confirmingDelete ? (
            <div className="flex items-center gap-2">
              <span className="text-muted-foreground text-sm">Delete?</span>
              <Button
                type="button"
                variant="destructive"
                size="sm"
                onClick={remove}
                disabled={busy}
              >
                Confirm
              </Button>
              <Button
                type="button"
                variant="ghost"
                size="sm"
                onClick={() => setConfirmingDelete(false)}
                disabled={busy}
              >
                Cancel
              </Button>
            </div>
          ) : (
            <Button
              type="button"
              variant="ghost"
              size="sm"
              className="text-destructive hover:text-destructive"
              onClick={() => setConfirmingDelete(true)}
              disabled={busy}
            >
              <Trash2 />
              Delete
            </Button>
          )}
          <div className="flex gap-2">
            <Button
              type="button"
              variant="outline"
              onClick={onClose}
              disabled={busy}
            >
              Cancel
            </Button>
            <Button type="submit" disabled={busy}>
              {busy ? "Saving…" : "Save"}
            </Button>
          </div>
        </DialogFooter>
      </form>
    </>
  );
}
