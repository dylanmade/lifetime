import { useState, type FormEvent } from "react";
import { enableEncryption } from "./api";

type Props = {
  onClose: () => void;
  onCompleted: () => void;
};

type DialogState =
  | { kind: "input"; error: string | null }
  | { kind: "working" }
  | { kind: "success"; recoveryFile: string; fingerprint: string };

export function EnableEncryption({ onClose, onCompleted }: Props) {
  const [state, setState] = useState<DialogState>({ kind: "input", error: null });
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

  return (
    <div className="modal-backdrop">
      <div className="modal modal-wide">
        {state.kind === "input" && (
          <form onSubmit={submit}>
            <h2>Enable encryption</h2>
            <p>
              Choose a passphrase. You'll need it every time you open Lifetime.
              If you forget it, only your recovery file can get your data back —
              we can't recover it for you.
            </p>
            <input
              type="password"
              value={passphrase}
              onChange={(e) => setPassphrase(e.target.value)}
              placeholder="Passphrase"
              autoFocus
              autoComplete="new-password"
            />
            <input
              type="password"
              value={confirm}
              onChange={(e) => setConfirm(e.target.value)}
              placeholder="Confirm passphrase"
              autoComplete="new-password"
            />
            {state.error && <p className="error">{state.error}</p>}
            <div className="modal-actions">
              <button type="button" onClick={onClose}>
                Cancel
              </button>
              <button
                type="submit"
                disabled={!passphrase || !confirm}
                className="primary"
              >
                Enable encryption
              </button>
            </div>
          </form>
        )}

        {state.kind === "working" && (
          <div>
            <h2>Encrypting your data…</h2>
            <p>
              Deriving keys and re-encrypting the local database. This usually
              takes a second or two.
            </p>
          </div>
        )}

        {state.kind === "success" && (
          <div>
            <h2>Save your recovery file</h2>
            <p>
              <strong>
                This is the only way to recover your data if you forget your
                passphrase.
              </strong>{" "}
              Store it somewhere safe — a password manager, an encrypted USB,
              or printed and kept offline. We do not keep a copy.
            </p>
            <p className="subtitle">
              Master key: <code>{state.fingerprint}</code>
            </p>
            <pre className="recovery-file">{state.recoveryFile}</pre>
            <button type="button" onClick={copyRecoveryFile}>
              {copied ? "Copied!" : "Copy to clipboard"}
            </button>
            <label className="acknowledge">
              <input
                type="checkbox"
                checked={acknowledged}
                onChange={(e) => setAcknowledged(e.target.checked)}
              />
              I've saved my recovery file somewhere safe.
            </label>
            <div className="modal-actions">
              <button
                type="button"
                onClick={onCompleted}
                disabled={!acknowledged}
                className="primary"
              >
                Done
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
