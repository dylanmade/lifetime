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

export type AppDuration = {
  app_name: string;
  bundle_id: string | null;
  active_seconds: number;
  idle_seconds: number;
};

export const getAppTotals = (
  startIso: string,
  endIso: string,
): Promise<AppDuration[]> => invoke("get_app_totals", { startIso, endIso });

export type HourActivity = {
  hour: number;
  active_seconds: number;
};

export const getHourlyActivity = (
  startIso: string,
  endIso: string,
): Promise<HourActivity[]> =>
  invoke("get_hourly_activity", { startIso, endIso });

export type AppSegment = {
  app_name: string;
  bundle_id: string | null;
  starts_at: string;
  ends_at: string;
  is_active: boolean;
};

export const getTimelineSegments = (
  startIso: string,
  endIso: string,
): Promise<AppSegment[]> =>
  invoke("get_timeline_segments", { startIso, endIso });

export type Activity = {
  id: string;
  device_id: string;
  starts_at: string;
  ends_at: string | null;
  kind: string;
  title?: string;
  description?: string | null;
};

export const createManualActivity = (input: {
  title: string;
  description: string | null;
  startsAtIso: string;
  endsAtIso: string | null;
}): Promise<Activity> =>
  invoke("create_manual_activity", {
    title: input.title,
    description: input.description,
    startsAtIso: input.startsAtIso,
    endsAtIso: input.endsAtIso,
  });

export const getActivitiesBetween = (
  startIso: string,
  endIso: string,
): Promise<Activity[]> =>
  invoke("get_activities_between", { startIso, endIso });

export const isAccessibilityGranted = (): Promise<boolean> =>
  invoke("accessibility_granted");

export const requestAccessibility = (): Promise<boolean> =>
  invoke("request_accessibility");
