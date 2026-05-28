import { useEffect, useState } from "react";
import {
  type AppStateInfo,
  isAccessibilityGranted,
  requestAccessibility,
} from "./api";
import { EnableEncryption } from "./EnableEncryption";

type Props = {
  appState: AppStateInfo;
  onStateChanged: () => void;
};

export function Settings({ appState, onStateChanged }: Props) {
  const [showEnable, setShowEnable] = useState(false);
  const [axGranted, setAxGranted] = useState<boolean | null>(null);
  const [axRequested, setAxRequested] = useState(false);

  const fingerprint =
    "fingerprint" in appState ? appState.fingerprint : null;

  useEffect(() => {
    isAccessibilityGranted().then(setAxGranted);
  }, []);

  async function handleAccessibility() {
    const granted = await requestAccessibility();
    setAxGranted(granted);
    setAxRequested(true);
  }

  return (
    <main className="container">
      <h1>Settings</h1>

      <section className="settings-section">
        <h2>Security</h2>
        {appState.status === "plaintext" && (
          <>
            <p>
              Your data is currently stored unencrypted on this device. Anyone
              with access to the disk (including some cloud backups) can read it.
              Enable encryption to protect your data with a passphrase.
            </p>
            <button type="button" onClick={() => setShowEnable(true)}>
              Enable encryption
            </button>
          </>
        )}
        {(appState.status === "locked" || appState.status === "unlocked") && (
          <p>
            Encryption is enabled.
            {fingerprint && (
              <>
                <br />
                Master key: <code>{fingerprint}</code>
              </>
            )}
          </p>
        )}
      </section>

      <section className="settings-section">
        <h2>Tracking</h2>
        <p>
          Window titles add useful detail to your timeline — "Safari — Hacker
          News" instead of just "Safari". They require Accessibility permission,
          which you grant in System Settings under Privacy &amp; Security.
        </p>
        {axGranted === null && (
          <p className="subtitle">Checking permission…</p>
        )}
        {axGranted !== null && (
          <>
            <p>
              Status:{" "}
              <strong>{axGranted ? "Granted" : "Not granted"}</strong>
            </p>
            {!axGranted && (
              <button type="button" onClick={handleAccessibility}>
                {axRequested ? "Check status" : "Grant access"}
              </button>
            )}
            {axGranted && axRequested && (
              <p className="subtitle">
                Window titles will appear in new observations. If they don't,
                restart Lifetime so the permission is fully picked up.
              </p>
            )}
          </>
        )}
      </section>

      {showEnable && (
        <EnableEncryption
          onClose={() => setShowEnable(false)}
          onCompleted={() => {
            setShowEnable(false);
            onStateChanged();
          }}
        />
      )}
    </main>
  );
}
