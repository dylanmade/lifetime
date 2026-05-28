import { invoke } from "@tauri-apps/api/core";

export type Observation = {
  id: string;
  device_id: string;
  recorded_at: string;
  kind: string;
  app_name?: string;
  bundle_id?: string | null;
  window_title?: string | null;
  is_active?: boolean;
  idle_seconds?: number;
};

export type AppStateInfo =
  | { status: "plaintext" }
  | { status: "locked"; fingerprint: string }
  | { status: "unlocked"; fingerprint: string };

export type EnableEncryptionResult = {
  recovery_file: string;
  fingerprint: string;
};

export const getAppState = (): Promise<AppStateInfo> => invoke("app_state");

export const unlock = (passphrase: string): Promise<void> =>
  invoke("unlock", { passphrase });

export const enableEncryption = (
  passphrase: string,
): Promise<EnableEncryptionResult> =>
  invoke("enable_encryption", { passphrase });

export const getRecentObservations = (limit: number): Promise<Observation[]> =>
  invoke("get_recent_observations", { limit });
