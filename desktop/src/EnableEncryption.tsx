import { useState, type FormEvent } from "react";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Checkbox } from "@/components/ui/checkbox";
import { Spinner } from "@/components/ui/spinner";
import { enableEncryption } from "./api";

type Props = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onCompleted: () => void;
};

type DialogState =
  | { kind: "input"; error: string | null }
  | { kind: "working" }
  | { kind: "success"; recoveryFile: string; fingerprint: string };

export function EnableEncryption({ open, onOpenChange, onCompleted }: Props) {
  const [state, setState] = useState<DialogState>({
    kind: "input",
    error: null,
  });
  const [passphrase, setPassphrase] = useState("");
  const [confirm, setConfirm] = useState("");
  const [acknowledged, setAcknowledged] = useState(false);
  const [copied, setCopied] = useState(false);

  async function submit(e: FormEvent) {
    e.preventDefault();
    if (passphrase !== confirm) {
      setState({ kind: "input", error: "Passphrases don't match." });
      return;
    }
    if (passphrase.length < 8) {
      setState({ kind: "input", error: "Use at least 8 characters." });
      return;
    }
    setState({ kind: "working" });
    try {
      const result = await enableEncryption(passphrase);
      setState({
        kind: "success",
        recoveryFile: result.recovery_file,
        fingerprint: result.fingerprint,
      });
    } catch (e) {
      setState({ kind: "input", error: String(e) });
    }
  }

  async function copyRecoveryFile() {
    if (state.kind !== "success") return;
    await navigator.clipboard.writeText(state.recoveryFile);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  }

  const dismissable = state.kind === "input";

  return (
    <Dialog
      open={open}
      onOpenChange={(o) => {
        if (dismissable) onOpenChange(o);
      }}
    >
      <DialogContent
        className="sm:max-w-lg"
        showCloseButton={dismissable}
        onEscapeKeyDown={(e) => {
          if (!dismissable) e.preventDefault();
        }}
        onInteractOutside={(e) => {
          if (!dismissable) e.preventDefault();
        }}
      >
        {state.kind === "input" && (
          <>
            <DialogHeader>
              <DialogTitle>Enable encryption</DialogTitle>
              <DialogDescription>
                Choose a passphrase. You'll need it every time you open
                Lifetime. If you forget it, only your recovery file can get your
                data back — we can't recover it for you.
              </DialogDescription>
            </DialogHeader>
            <form onSubmit={submit} className="space-y-4">
              <div className="space-y-1.5">
                <Label htmlFor="enc-passphrase">Passphrase</Label>
                <Input
                  id="enc-passphrase"
                  type="password"
                  value={passphrase}
                  onChange={(e) => setPassphrase(e.target.value)}
                  autoFocus
                  autoComplete="new-password"
                />
              </div>
              <div className="space-y-1.5">
                <Label htmlFor="enc-confirm">Confirm passphrase</Label>
                <Input
                  id="enc-confirm"
                  type="password"
                  value={confirm}
                  onChange={(e) => setConfirm(e.target.value)}
                  autoComplete="new-password"
                />
              </div>
              {state.error && (
                <Alert variant="destructive">
                  <AlertDescription>{state.error}</AlertDescription>
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
                <Button type="submit" disabled={!passphrase || !confirm}>
                  Enable encryption
                </Button>
              </DialogFooter>
            </form>
          </>
        )}

        {state.kind === "working" && (
          <>
            <DialogHeader>
              <DialogTitle>Encrypting your data…</DialogTitle>
              <DialogDescription>
                Deriving keys and re-encrypting the local database. This usually
                takes a second or two.
              </DialogDescription>
            </DialogHeader>
            <div className="flex items-center justify-center py-4">
              <Spinner className="text-primary size-6" />
            </div>
          </>
        )}

        {state.kind === "success" && (
          <>
            <DialogHeader>
              <DialogTitle>Save your recovery file</DialogTitle>
              <DialogDescription>
                <strong>
                  This is the only way to recover your data if you forget your
                  passphrase.
                </strong>{" "}
                Store it somewhere safe — a password manager, an encrypted USB,
                or printed and kept offline. We do not keep a copy.
              </DialogDescription>
            </DialogHeader>
            <div className="space-y-3">
              <p className="text-muted-foreground text-xs">
                Master key:{" "}
                <code className="bg-muted rounded px-1.5 py-0.5">
                  {state.fingerprint}
                </code>
              </p>
              <pre className="bg-muted rounded-md border p-3 font-mono text-xs break-all whitespace-pre-wrap select-text">
                {state.recoveryFile}
              </pre>
              <Button
                type="button"
                variant="outline"
                onClick={copyRecoveryFile}
                className="w-full"
              >
                {copied ? "Copied!" : "Copy to clipboard"}
              </Button>
              <div className="flex items-start gap-2">
                <Checkbox
                  id="ack"
                  checked={acknowledged}
                  onCheckedChange={(checked) =>
                    setAcknowledged(checked === true)
                  }
                />
                <Label
                  htmlFor="ack"
                  className="text-sm leading-tight font-normal"
                >
                  I've saved my recovery file somewhere safe.
                </Label>
              </div>
            </div>
            <DialogFooter>
              <Button onClick={onCompleted} disabled={!acknowledged}>
                Done
              </Button>
            </DialogFooter>
          </>
        )}
      </DialogContent>
    </Dialog>
  );
}
