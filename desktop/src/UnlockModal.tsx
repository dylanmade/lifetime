import { useState, type FormEvent } from "react";
import { Lock } from "lucide-react";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { unlock } from "./api";

type Props = {
  fingerprint: string;
  onUnlocked: () => void;
};

export function UnlockModal({ fingerprint, onUnlocked }: Props) {
  const [passphrase, setPassphrase] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function submit(e: FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      await unlock(passphrase);
      onUnlocked();
    } catch (e) {
      setError(String(e));
      setPassphrase("");
    } finally {
      setBusy(false);
    }
  }

  return (
    <Dialog open onOpenChange={() => {}}>
      <DialogContent
        showCloseButton={false}
        className="sm:max-w-md"
        onEscapeKeyDown={(e) => e.preventDefault()}
        onInteractOutside={(e) => e.preventDefault()}
      >
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Lock className="h-4 w-4" /> Lifetime is locked
          </DialogTitle>
          <DialogDescription>
            Enter your passphrase to unlock your data.
            <br />
            Master key:{" "}
            <code className="bg-muted rounded px-1.5 py-0.5 text-xs">
              {fingerprint}
            </code>
          </DialogDescription>
        </DialogHeader>
        <form onSubmit={submit} className="space-y-4">
          <div className="space-y-1.5">
            <Label htmlFor="unlock-passphrase">Passphrase</Label>
            <Input
              id="unlock-passphrase"
              type="password"
              value={passphrase}
              onChange={(e) => setPassphrase(e.target.value)}
              autoFocus
              autoComplete="current-password"
            />
          </div>
          {error && (
            <Alert variant="destructive">
              <AlertDescription>{error}</AlertDescription>
            </Alert>
          )}
          <div className="flex justify-end">
            <Button type="submit" disabled={busy || !passphrase}>
              {busy ? "Unlocking…" : "Unlock"}
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  );
}
