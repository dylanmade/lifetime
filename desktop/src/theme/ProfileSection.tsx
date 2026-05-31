import { useEffect, useState } from "react";
import { Pencil, Plus, Save, Trash2 } from "lucide-react";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { useTheme } from "./ThemeProvider";

const CUSTOM_VALUE = "__custom__";

export function ProfileSection() {
  const {
    profiles,
    activeProfileId,
    isDirty,
    loadProfile,
    saveAsNewProfile,
    saveActiveProfile,
    renameActiveProfile,
    deleteProfile,
    clearActiveProfile,
  } = useTheme();

  const [saveAsOpen, setSaveAsOpen] = useState(false);
  const [renameOpen, setRenameOpen] = useState(false);
  const [confirmDeleteOpen, setConfirmDeleteOpen] = useState(false);

  const activeProfile = profiles.find((p) => p.id === activeProfileId);

  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-muted-foreground text-xs font-medium tracking-wider uppercase">
          Profile
        </CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="flex flex-wrap items-center gap-3">
          <Select
            value={activeProfileId ?? CUSTOM_VALUE}
            onValueChange={(v) => {
              if (v === CUSTOM_VALUE) {
                clearActiveProfile();
              } else {
                loadProfile(v).catch(() => {
                  // Best-effort: provider already resets state on error.
                });
              }
            }}
          >
            <SelectTrigger className="w-56">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value={CUSTOM_VALUE}>
                {activeProfileId
                  ? "(Switch to custom)"
                  : "Custom (no profile loaded)"}
              </SelectItem>
              {profiles.map((p) => (
                <SelectItem key={p.id} value={p.id}>
                  {p.name}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          {activeProfileId && isDirty && (
            <span className="text-muted-foreground text-xs">
              · Unsaved changes
            </span>
          )}
        </div>

        <div className="flex flex-wrap gap-2">
          {activeProfileId && (
            <Button
              size="sm"
              onClick={() => saveActiveProfile().catch(() => {})}
              disabled={!isDirty}
            >
              <Save className="mr-2 h-4 w-4" />
              Save
            </Button>
          )}
          <Button
            size="sm"
            variant="outline"
            onClick={() => setSaveAsOpen(true)}
          >
            <Plus className="mr-2 h-4 w-4" />
            Save as new
          </Button>
          {activeProfileId && (
            <>
              <Button
                size="sm"
                variant="outline"
                onClick={() => setRenameOpen(true)}
              >
                <Pencil className="mr-2 h-4 w-4" />
                Rename
              </Button>
              <Button
                size="sm"
                variant="outline"
                onClick={() => setConfirmDeleteOpen(true)}
                className="text-destructive hover:text-destructive"
              >
                <Trash2 className="mr-2 h-4 w-4" />
                Delete
              </Button>
            </>
          )}
        </div>
      </CardContent>

      <NameDialog
        open={saveAsOpen}
        title="Save as new profile"
        confirmLabel="Save"
        defaultValue=""
        onCancel={() => setSaveAsOpen(false)}
        onConfirm={async (name) => {
          await saveAsNewProfile(name);
          setSaveAsOpen(false);
        }}
      />
      <NameDialog
        open={renameOpen}
        title="Rename profile"
        confirmLabel="Rename"
        defaultValue={activeProfile?.name ?? ""}
        onCancel={() => setRenameOpen(false)}
        onConfirm={async (name) => {
          await renameActiveProfile(name);
          setRenameOpen(false);
        }}
      />
      <ConfirmDialog
        open={confirmDeleteOpen}
        title="Delete profile?"
        message={`This will permanently delete "${activeProfile?.name ?? "this profile"}". Your current local appearance settings will stay applied.`}
        onCancel={() => setConfirmDeleteOpen(false)}
        onConfirm={async () => {
          if (activeProfileId) await deleteProfile(activeProfileId);
          setConfirmDeleteOpen(false);
        }}
      />
    </Card>
  );
}

function NameDialog({
  open,
  title,
  confirmLabel,
  defaultValue,
  onCancel,
  onConfirm,
}: {
  open: boolean;
  title: string;
  confirmLabel: string;
  defaultValue: string;
  onCancel: () => void;
  onConfirm: (name: string) => Promise<void>;
}) {
  const [value, setValue] = useState(defaultValue);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (open) {
      setValue(defaultValue);
      setError(null);
      setBusy(false);
    }
  }, [open, defaultValue]);

  const trimmed = value.trim();
  const submit = async () => {
    if (!trimmed || busy) return;
    setBusy(true);
    setError(null);
    try {
      await onConfirm(trimmed);
    } catch (e) {
      setError(formatError(e));
      setBusy(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={(o) => !o && !busy && onCancel()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
        </DialogHeader>
        <Input
          value={value}
          onChange={(e) => setValue(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") submit();
          }}
          placeholder="Profile name"
          autoFocus
          disabled={busy}
        />
        {error && (
          <Alert variant="destructive">
            <AlertDescription>{error}</AlertDescription>
          </Alert>
        )}
        <DialogFooter>
          <Button variant="outline" onClick={onCancel} disabled={busy}>
            Cancel
          </Button>
          <Button onClick={submit} disabled={!trimmed || busy}>
            {confirmLabel}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function ConfirmDialog({
  open,
  title,
  message,
  onCancel,
  onConfirm,
}: {
  open: boolean;
  title: string;
  message: string;
  onCancel: () => void;
  onConfirm: () => Promise<void>;
}) {
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    if (open) setBusy(false);
  }, [open]);
  return (
    <Dialog open={open} onOpenChange={(o) => !o && !busy && onCancel()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
        </DialogHeader>
        <p className="text-muted-foreground text-sm">{message}</p>
        <DialogFooter>
          <Button variant="outline" onClick={onCancel} disabled={busy}>
            Cancel
          </Button>
          <Button
            variant="destructive"
            onClick={async () => {
              setBusy(true);
              try {
                await onConfirm();
              } catch {
                setBusy(false);
              }
            }}
            disabled={busy}
          >
            Delete
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function formatError(e: unknown): string {
  const msg = String(e);
  // SQLite UNIQUE constraint failure surfaces from rusqlite as a message
  // mentioning UNIQUE — translate to something user-friendly.
  if (msg.includes("UNIQUE") || msg.toLowerCase().includes("unique"))
    return "A profile with that name already exists.";
  return msg;
}
