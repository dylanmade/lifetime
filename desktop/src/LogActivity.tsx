import { useState, type FormEvent } from "react";
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
import { Alert, AlertDescription } from "@/components/ui/alert";
import { createManualActivity } from "./api";
import { isSameDay, toDateTimeLocalValue } from "@/lib/datetime";

type Props = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  defaultDate: Date;
  onCreated: () => void;
};

function defaultStartTime(forDate: Date): Date {
  const now = new Date();
  if (isSameDay(forDate, now)) {
    return new Date(now.getTime() - 60 * 60 * 1000);
  }
  const d = new Date(forDate);
  d.setHours(12, 0, 0, 0);
  return d;
}

export function LogActivity({
  open,
  onOpenChange,
  defaultDate,
  onCreated,
}: Props) {
  const [title, setTitle] = useState("");
  const [description, setDescription] = useState("");
  const [startLocal, setStartLocal] = useState(() =>
    toDateTimeLocalValue(defaultStartTime(defaultDate)),
  );
  const [endLocal, setEndLocal] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function submit(e: FormEvent) {
    e.preventDefault();
    setError(null);

    const trimmedTitle = title.trim();
    if (!trimmedTitle) {
      setError("Title is required.");
      return;
    }
    if (!startLocal) {
      setError("Start time is required.");
      return;
    }
    const start = new Date(startLocal);
    if (Number.isNaN(start.getTime())) {
      setError("Invalid start time.");
      return;
    }
    let end: Date | null = null;
    if (endLocal) {
      end = new Date(endLocal);
      if (Number.isNaN(end.getTime())) {
        setError("Invalid end time.");
        return;
      }
      if (end <= start) {
        setError("End time must be after start time.");
        return;
      }
    }

    setBusy(true);
    try {
      await createManualActivity({
        title: trimmedTitle,
        description: description.trim() || null,
        startsAtIso: start.toISOString(),
        endsAtIso: end ? end.toISOString() : null,
      });
      // Reset for next open
      setTitle("");
      setDescription("");
      setEndLocal("");
      onCreated();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>Log activity</DialogTitle>
        </DialogHeader>
        <form onSubmit={submit} className="space-y-4">
          <div className="space-y-1.5">
            <Label htmlFor="act-title">What did you do?</Label>
            <Input
              id="act-title"
              type="text"
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              autoFocus
              maxLength={200}
            />
          </div>
          <div className="grid grid-cols-2 gap-3">
            <div className="space-y-1.5">
              <Label htmlFor="act-start">Start</Label>
              <Input
                id="act-start"
                type="datetime-local"
                value={startLocal}
                onChange={(e) => setStartLocal(e.target.value)}
                required
              />
            </div>
            <div className="space-y-1.5">
              <Label htmlFor="act-end" className="flex items-baseline gap-1.5">
                End{" "}
                <span className="text-muted-foreground text-xs font-normal">
                  optional
                </span>
              </Label>
              <Input
                id="act-end"
                type="datetime-local"
                value={endLocal}
                onChange={(e) => setEndLocal(e.target.value)}
              />
            </div>
          </div>
          <div className="space-y-1.5">
            <Label htmlFor="act-notes">Notes</Label>
            <Textarea
              id="act-notes"
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
          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={() => onOpenChange(false)}
            >
              Cancel
            </Button>
            <Button type="submit" disabled={busy || !title.trim()}>
              {busy ? "Saving…" : "Save"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
