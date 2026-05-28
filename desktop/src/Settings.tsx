import { useState } from "react";
import { type AppStateInfo } from "./api";
import { EnableEncryption } from "./EnableEncryption";

type Props = {
  appState: AppStateInfo;
  onStateChanged: () => void;
};

export function Settings({ appState, onStateChanged }: Props) {
  const [showEnable, setShowEnable] = useState(false);
  const fingerprint =
    "fingerprint" in appState ? appState.fingerprint : null;

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
