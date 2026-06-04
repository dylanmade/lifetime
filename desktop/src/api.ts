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

export type ActivitySource = "manual" | "auto";

// The unified activity read-model. Auto-only measured fields (app_name,
// bundle_id, window_title, is_active) are null for manual activities.
export type ResolvedActivity = {
  id: string;
  source: ActivitySource;
  starts_at: string;
  ends_at: string | null;
  title: string;
  category: string | null; // null ⇒ Unassigned
  description: string | null;
  app_name: string | null;
  bundle_id: string | null;
  window_title: string | null;
  is_active: boolean | null;
};

export const createManualActivity = (input: {
  title: string;
  description: string | null;
  startsAtIso: string;
  endsAtIso: string | null;
}): Promise<void> =>
  invoke("create_manual_activity", {
    title: input.title,
    description: input.description,
    startsAtIso: input.startsAtIso,
    endsAtIso: input.endsAtIso,
  });

export const getActivitiesBetween = (
  startIso: string,
  endIso: string,
): Promise<ResolvedActivity[]> =>
  invoke("get_activities_between", { startIso, endIso });

// Partial edit. Only the provided fields are written as annotation events.
// For description/category an empty string clears the value; endsAtIso "" makes
// a manual activity open-ended. title/time are rejected on auto activities.
export const updateActivity = (
  id: string,
  fields: {
    title?: string;
    description?: string;
    category?: string;
    startsAtIso?: string;
    endsAtIso?: string;
  },
): Promise<void> => invoke("update_activity", { id, ...fields });

export const deleteActivity = (id: string): Promise<void> =>
  invoke("delete_activity", { id });

export const isAccessibilityGranted = (): Promise<boolean> =>
  invoke("accessibility_granted");

export const requestAccessibility = (): Promise<boolean> =>
  invoke("request_accessibility");

export type ThemeProfileSummary = {
  id: string;
  name: string;
  updated_at: string;
};

export type ThemeProfile = {
  id: string;
  name: string;
  created_at: string;
  updated_at: string;
  data: string;
};

export const listThemeProfiles = (): Promise<ThemeProfileSummary[]> =>
  invoke("list_theme_profiles");

export const getThemeProfile = (id: string): Promise<ThemeProfile | null> =>
  invoke("get_theme_profile", { id });

export const createThemeProfile = (
  name: string,
  data: string,
): Promise<ThemeProfile> => invoke("create_theme_profile", { name, data });

export const updateThemeProfile = (
  id: string,
  name: string,
  data: string,
): Promise<ThemeProfile> => invoke("update_theme_profile", { id, name, data });

export const deleteThemeProfile = (id: string): Promise<void> =>
  invoke("delete_theme_profile", { id });
