import { useEffect, useState } from "react";
import { Lock, ShieldCheck, Eye } from "lucide-react";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Button } from "@/components/ui/button";
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

  const fingerprint = "fingerprint" in appState ? appState.fingerprint : null;
  const isEncrypted =
    appState.status === "locked" || appState.status === "unlocked";

  useEffect(() => {
    isAccessibilityGranted().then(setAxGranted);
  }, []);

  async function handleAccessibility() {
    const granted = await requestAccessibility();
    setAxGranted(granted);
    setAxRequested(true);
  }

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-semibold tracking-tight">Settings</h1>
      </div>

      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            {isEncrypted ? (
              <ShieldCheck className="text-primary size-4" />
            ) : (
              <Lock className="text-muted-foreground size-4" />
            )}
            Security
          </CardTitle>
          {appState.status === "plaintext" ? (
            <CardDescription>
              Your data is currently stored unencrypted on this device. Anyone
              with access to the disk (including some cloud backups) can read
              it. Enable encryption to protect your data with a passphrase.
            </CardDescription>
          ) : (
            <CardDescription>
              Encryption is enabled. Your data is encrypted at rest with a key
              derived from your passphrase.
            </CardDescription>
          )}
        </CardHeader>
        <CardContent>
          {appState.status === "plaintext" && (
            <Button onClick={() => setShowEnable(true)}>
              Enable encryption
            </Button>
          )}
          {isEncrypted && fingerprint && (
            <p className="text-muted-foreground text-sm">
              Master key:{" "}
              <code className="bg-muted rounded px-1.5 py-0.5 text-xs">
                {fingerprint}
              </code>
            </p>
          )}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Eye className="text-muted-foreground size-4" />
            Tracking
          </CardTitle>
          <CardDescription>
            Window titles add useful detail to your timeline — "Safari — Hacker
            News" instead of just "Safari". They require Accessibility
            permission, which you grant in System Settings under Privacy &amp;
            Security.
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-3">
          {axGranted === null && (
            <p className="text-muted-foreground text-sm">
              Checking permission…
            </p>
          )}
          {axGranted !== null && (
            <>
              <p className="text-sm">
                Status:{" "}
                <span
                  className={
                    axGranted
                      ? "text-primary font-medium"
                      : "text-muted-foreground"
                  }
                >
                  {axGranted ? "Granted" : "Not granted"}
                </span>
              </p>
              {!axGranted && (
                <Button variant="outline" onClick={handleAccessibility}>
                  {axRequested ? "Check status" : "Grant access"}
                </Button>
              )}
              {axGranted && axRequested && (
                <p className="text-muted-foreground text-sm">
                  Window titles will appear in new observations. If they don't,
                  restart Lifetime so the permission is fully picked up.
                </p>
              )}
            </>
          )}
        </CardContent>
      </Card>

      <EnableEncryption
        open={showEnable}
        onOpenChange={setShowEnable}
        onCompleted={() => {
          setShowEnable(false);
          onStateChanged();
        }}
      />
    </div>
  );
}
