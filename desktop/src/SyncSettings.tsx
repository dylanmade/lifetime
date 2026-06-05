import { useEffect, useState } from "react";
import { Share2 } from "lucide-react";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Label } from "@/components/ui/label";
import { Alert, AlertDescription } from "@/components/ui/alert";
import {
  type AppStateInfo,
  type SyncStatus,
  importRecoveryAndPair,
  syncStatus,
  syncWith,
} from "./api";

type Props = {
  appState: AppStateInfo;
  onStateChanged: () => void;
};

export function SyncSettings({ appState, onStateChanged }: Props) {
  const [status, setStatus] = useState<SyncStatus | null>(null);

  // Poll the sync service for the listening port + last-sync info.
  useEffect(() => {
    let cancelled = false;
    const load = () =>
      syncStatus()
        .then((s) => !cancelled && setStatus(s))
        .catch(() => {});
    load();
    const id = setInterval(load, 5000);
    return () => {
      cancelled = true;
      clearInterval(id);
    };
  }, []);

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <Share2 className="text-muted-foreground size-4" />
          Sync &amp; devices
        </CardTitle>
        <CardDescription>
          Sync keeps your devices' data in step over the local network,
          peer-to-peer. It requires encryption — the shared master key secures
          the connection. Each device keeps its own timeline; viewing them
          together is your choice.
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        {appState.status === "plaintext" && (
          <PairForm onPaired={onStateChanged} />
        )}
        {appState.status === "locked" && (
          <p className="text-muted-foreground text-sm">
            Unlock Lifetime to sync.
          </p>
        )}
        {appState.status === "unlocked" && <UnlockedSync status={status} />}
      </CardContent>
    </Card>
  );
}

function PairForm({ onPaired }: { onPaired: () => void }) {
  const [recovery, setRecovery] = useState("");
  const [passphrase, setPassphrase] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function pair() {
    setError(null);
    if (!recovery.trim()) {
      return setError("Paste the other device's recovery file.");
    }
    if (passphrase.length < 8) {
      return setError("Use a passphrase of at least 8 characters.");
    }
    setBusy(true);
    try {
      await importRecoveryAndPair(recovery.trim(), passphrase);
      onPaired();
    } catch (e) {
      setError(String(e));
      setBusy(false);
    }
  }

  return (
    <div className="space-y-3">
      <p className="text-sm">
        To join another device's data, paste its recovery file and choose a
        passphrase for this device. (To start a fresh vault instead, enable
        encryption above.)
      </p>
      <div className="space-y-1.5">
        <Label htmlFor="pair-recovery">Recovery file</Label>
        <Textarea
          id="pair-recovery"
          value={recovery}
          onChange={(e) => setRecovery(e.target.value)}
          rows={3}
          className="font-mono text-xs"
          placeholder="LIFETIME-RECOVERY-V1 …"
          spellCheck={false}
        />
      </div>
      <div className="space-y-1.5">
        <Label htmlFor="pair-pass">Passphrase for this device</Label>
        <Input
          id="pair-pass"
          type="password"
          value={passphrase}
          onChange={(e) => setPassphrase(e.target.value)}
          autoComplete="new-password"
        />
      </div>
      {error && (
        <Alert variant="destructive">
          <AlertDescription>{error}</AlertDescription>
        </Alert>
      )}
      <Button onClick={pair} disabled={busy}>
        {busy ? "Pairing…" : "Pair this device"}
      </Button>
    </div>
  );
}

function UnlockedSync({ status }: { status: SyncStatus | null }) {
  const [host, setHost] = useState("");
  const [port, setPort] = useState("");
  // Tag of the in-flight sync (a peer device_id, or "manual"); null when idle.
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<SyncStatus | null>(null);

  async function doSync(h: string, p: number, tag: string) {
    setError(null);
    setResult(null);
    setBusy(tag);
    try {
      setResult(await syncWith(h, p));
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  }

  function connectManual() {
    const p = Number(port);
    if (!host.trim() || !Number.isInteger(p) || p <= 0 || p > 65535) {
      return setError("Enter the peer's host and port.");
    }
    doSync(host.trim(), p, "manual");
  }

  const live = result ?? status;
  const peers = status?.peers ?? [];

  return (
    <div className="space-y-4">
      <p className="text-sm">
        This device accepts sync connections on port{" "}
        <code className="bg-muted rounded px-1.5 py-0.5 text-xs tabular-nums">
          {status?.listening_port ?? "…"}
        </code>
        .
      </p>

      <div className="space-y-1.5">
        <Label>Discovered on your network</Label>
        {peers.length === 0 ? (
          <p className="text-muted-foreground text-sm">
            No devices found yet — they appear automatically when another
            Lifetime device is unlocked on the same network.
          </p>
        ) : (
          <ul className="space-y-1">
            {peers.map((peer) => (
              <li
                key={peer.device_id}
                className="flex items-center justify-between gap-3 rounded-md border px-3 py-2"
              >
                <span className="font-mono text-xs tabular-nums">
                  {peer.host}:{peer.port}
                </span>
                <Button
                  size="sm"
                  variant="outline"
                  disabled={busy !== null}
                  onClick={() => doSync(peer.host, peer.port, peer.device_id)}
                >
                  {busy === peer.device_id ? "Syncing…" : "Sync"}
                </Button>
              </li>
            ))}
          </ul>
        )}
      </div>

      <div className="space-y-1.5">
        <Label>Or connect manually</Label>
        <div className="grid grid-cols-[1fr_auto_auto] items-end gap-2">
          <Input
            value={host}
            onChange={(e) => setHost(e.target.value)}
            placeholder="192.168.1.42"
            aria-label="Peer host"
          />
          <Input
            value={port}
            onChange={(e) => setPort(e.target.value)}
            placeholder="port"
            inputMode="numeric"
            aria-label="Peer port"
            className="w-24 tabular-nums"
          />
          <Button
            variant="outline"
            onClick={connectManual}
            disabled={busy !== null}
          >
            {busy === "manual" ? "Syncing…" : "Sync"}
          </Button>
        </div>
      </div>

      {error && (
        <Alert variant="destructive">
          <AlertDescription>{error}</AlertDescription>
        </Alert>
      )}

      {live?.last_synced_at && (
        <p className="text-muted-foreground text-sm">
          Last sync with{" "}
          <span className="text-foreground font-medium">{live.last_peer}</span>{" "}
          · sent <span className="tabular-nums">{live.last_sent}</span>,
          received <span className="tabular-nums">{live.last_received}</span>
          {live.last_error && (
            <span className="text-destructive"> · {live.last_error}</span>
          )}
        </p>
      )}
    </div>
  );
}
