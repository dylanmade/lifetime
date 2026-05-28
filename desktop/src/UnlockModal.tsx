import { useState, type FormEvent } from "react";
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
    <div className="modal-backdrop">
      <form className="modal" onSubmit={submit}>
        <h2>Lifetime is locked</h2>
        <p className="subtitle">
          Enter your passphrase to unlock your data.
          <br />
          Master key: <code>{fingerprint}</code>
        </p>
        <input
          type="password"
          value={passphrase}
          onChange={(e) => setPassphrase(e.target.value)}
          placeholder="Passphrase"
          autoFocus
          autoComplete="current-password"
        />
        {error && <p className="error">{error}</p>}
        <div className="modal-actions">
          <button type="submit" disabled={busy || !passphrase} className="primary">
            {busy ? "Unlocking…" : "Unlock"}
          </button>
        </div>
      </form>
    </div>
  );
}
