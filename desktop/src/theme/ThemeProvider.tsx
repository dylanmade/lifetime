import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import {
  MONO_FONTS,
  SANS_FONTS,
  type ColorToken,
  type FontOption,
} from "./tokens";
import {
  googleFontFamilyFromId,
  googleFontStack,
  isGoogleFontId,
  loadGoogleFont,
} from "./googleFonts";
import {
  createThemeProfile,
  deleteThemeProfile,
  getThemeProfile,
  listThemeProfiles,
  updateThemeProfile,
  type ThemeProfileSummary,
} from "../api";

export type ThemeMode = "light" | "dark" | "system";

export type ColorOverrides = Partial<Record<ColorToken, string>>;

export type ThemeOverrides = {
  fontSans?: string;
  fontHeading?: string;
  fontMono?: string;
  radiusRem?: number;
  colors: { light: ColorOverrides; dark: ColorOverrides };
};

type ThemeState = {
  mode: ThemeMode;
  overrides: ThemeOverrides;
};

// Wire format for the profile's `data` JSON blob. Same shape as ThemeState.
type ProfileData = ThemeState;

const STORAGE_KEY = "lifetime.theme.v1";
const ACTIVE_PROFILE_KEY = "lifetime.theme.activeProfileId.v1";

const emptyOverrides = (): ThemeOverrides => ({
  colors: { light: {}, dark: {} },
});

function loadState(): ThemeState {
  if (typeof localStorage === "undefined") {
    return { mode: "system", overrides: emptyOverrides() };
  }
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return { mode: "system", overrides: emptyOverrides() };
    const parsed = JSON.parse(raw) as Partial<ThemeState>;
    return {
      mode: parsed.mode ?? "system",
      overrides: {
        ...emptyOverrides(),
        ...(parsed.overrides ?? {}),
        colors: {
          light: parsed.overrides?.colors?.light ?? {},
          dark: parsed.overrides?.colors?.dark ?? {},
        },
      },
    };
  } catch {
    return { mode: "system", overrides: emptyOverrides() };
  }
}

function persistState(state: ThemeState) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
  } catch {
    // Quota / private mode — fail silently; user can re-apply at any time.
  }
}

function serializeProfile(state: ThemeState): string {
  // Stable order, no whitespace — used both for storage and dirty comparison.
  return JSON.stringify(state);
}

function resolveSystemMode(): "light" | "dark" {
  if (typeof window === "undefined" || !window.matchMedia) return "light";
  return window.matchMedia("(prefers-color-scheme: dark)").matches
    ? "dark"
    : "light";
}

// Resolve a font id (either a bundled FontOption.id or a "g:Family Name"
// Google Fonts id) into a CSS font-family stack. Google Fonts are lazily
// loaded via <link> injection the first time we resolve them.
function fontStack(id: string | undefined, bundled: FontOption[]): string {
  if (!id) return bundled[0].stack;
  if (isGoogleFontId(id)) {
    const family = googleFontFamilyFromId(id);
    if (family) {
      loadGoogleFont(family);
      return googleFontStack(family);
    }
  }
  return bundled.find((f) => f.id === id)?.stack ?? bundled[0].stack;
}

type ThemeContextValue = {
  mode: ThemeMode;
  activeMode: "light" | "dark";
  overrides: ThemeOverrides;
  setMode: (mode: ThemeMode) => void;
  setColor: (token: ColorToken, hex: string) => void;
  clearColor: (token: ColorToken) => void;
  setRadius: (rem: number) => void;
  setFontSans: (id: string) => void;
  setFontHeading: (id: string) => void;
  setFontMono: (id: string) => void;
  reset: () => void;
  // Profile management
  profiles: ThemeProfileSummary[];
  activeProfileId: string | null;
  isDirty: boolean;
  refreshProfiles: () => Promise<void>;
  loadProfile: (id: string) => Promise<void>;
  saveAsNewProfile: (name: string) => Promise<void>;
  saveActiveProfile: () => Promise<void>;
  renameActiveProfile: (name: string) => Promise<void>;
  deleteProfile: (id: string) => Promise<void>;
  clearActiveProfile: () => void;
};

const ThemeContext = createContext<ThemeContextValue | null>(null);

// Apply overrides + mode to the DOM. Called whenever state changes.
function applyToDom(state: ThemeState, activeMode: "light" | "dark") {
  if (typeof document === "undefined") return;
  const root = document.documentElement;

  root.classList.toggle("dark", activeMode === "dark");

  const previouslyApplied = root.dataset.themeAppliedKeys?.split(",") ?? [];
  for (const key of previouslyApplied) {
    if (key) root.style.removeProperty(key);
  }

  const writtenKeys: string[] = [];

  const colors = state.overrides.colors[activeMode];
  for (const [token, hex] of Object.entries(colors)) {
    if (hex) {
      const prop = `--${token}`;
      root.style.setProperty(prop, hex);
      writtenKeys.push(prop);
    }
  }

  if (state.overrides.radiusRem !== undefined) {
    root.style.setProperty("--radius", `${state.overrides.radiusRem}rem`);
    writtenKeys.push("--radius");
  }

  const sans = fontStack(state.overrides.fontSans, SANS_FONTS);
  const heading = fontStack(state.overrides.fontHeading, SANS_FONTS);
  const mono = fontStack(state.overrides.fontMono, MONO_FONTS);
  if (state.overrides.fontSans) {
    root.style.setProperty("--font-sans", sans);
    writtenKeys.push("--font-sans");
  }
  if (state.overrides.fontHeading) {
    root.style.setProperty("--font-heading", heading);
    writtenKeys.push("--font-heading");
  }
  if (state.overrides.fontMono) {
    root.style.setProperty("--font-mono", mono);
    writtenKeys.push("--font-mono");
  }

  root.dataset.themeAppliedKeys = writtenKeys.join(",");
}

export function ThemeProvider({ children }: { children: ReactNode }) {
  const [state, setState] = useState<ThemeState>(() => loadState());
  const [systemMode, setSystemMode] = useState<"light" | "dark">(() =>
    resolveSystemMode(),
  );
  const [profiles, setProfiles] = useState<ThemeProfileSummary[]>([]);
  const [activeProfileId, setActiveProfileId] = useState<string | null>(() =>
    typeof localStorage !== "undefined"
      ? localStorage.getItem(ACTIVE_PROFILE_KEY)
      : null,
  );
  // Serialized snapshot of the profile when it was loaded; used to detect
  // unsaved local edits.
  const [activeProfileSnapshot, setActiveProfileSnapshot] = useState<
    string | null
  >(null);

  const activeMode = state.mode === "system" ? systemMode : state.mode;

  useEffect(() => {
    if (typeof window === "undefined" || !window.matchMedia) return;
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const onChange = () => setSystemMode(mq.matches ? "dark" : "light");
    mq.addEventListener("change", onChange);
    return () => mq.removeEventListener("change", onChange);
  }, []);

  useEffect(() => {
    applyToDom(state, activeMode);
  }, [state, activeMode]);

  const isFirstRender = useRef(true);
  useEffect(() => {
    if (isFirstRender.current) {
      isFirstRender.current = false;
      return;
    }
    persistState(state);
  }, [state]);

  // Persist active profile id reference across sessions.
  useEffect(() => {
    if (typeof localStorage === "undefined") return;
    if (activeProfileId) {
      localStorage.setItem(ACTIVE_PROFILE_KEY, activeProfileId);
    } else {
      localStorage.removeItem(ACTIVE_PROFILE_KEY);
    }
  }, [activeProfileId]);

  const refreshProfiles = useCallback(async () => {
    try {
      const list = await listThemeProfiles();
      setProfiles(list);
    } catch {
      setProfiles([]);
    }
  }, []);

  // Load profiles on mount + rehydrate snapshot if there's a saved active id.
  useEffect(() => {
    let cancelled = false;
    (async () => {
      await refreshProfiles();
      if (cancelled || !activeProfileId) return;
      try {
        const profile = await getThemeProfile(activeProfileId);
        if (profile && !cancelled) {
          setActiveProfileSnapshot(profile.data);
        }
      } catch {
        // Profile may have been deleted on another device; just forget it.
        if (!cancelled) setActiveProfileId(null);
      }
    })();
    return () => {
      cancelled = true;
    };
    // We only want this to run on mount; refreshProfiles is stable, the
    // activeProfileId rehydrate is a one-time concern.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const isDirty = useMemo(() => {
    if (activeProfileId === null) return false;
    if (activeProfileSnapshot === null) return false;
    return serializeProfile(state) !== activeProfileSnapshot;
  }, [activeProfileId, activeProfileSnapshot, state]);

  const setMode = useCallback((mode: ThemeMode) => {
    setState((s) => ({ ...s, mode }));
  }, []);

  const setColor = useCallback(
    (token: ColorToken, hex: string) => {
      setState((s) => ({
        ...s,
        overrides: {
          ...s.overrides,
          colors: {
            ...s.overrides.colors,
            [activeMode]: { ...s.overrides.colors[activeMode], [token]: hex },
          },
        },
      }));
    },
    [activeMode],
  );

  const clearColor = useCallback(
    (token: ColorToken) => {
      setState((s) => {
        const next = { ...s.overrides.colors[activeMode] };
        delete next[token];
        return {
          ...s,
          overrides: {
            ...s.overrides,
            colors: { ...s.overrides.colors, [activeMode]: next },
          },
        };
      });
    },
    [activeMode],
  );

  const setRadius = useCallback((rem: number) => {
    setState((s) => ({
      ...s,
      overrides: { ...s.overrides, radiusRem: rem },
    }));
  }, []);

  const setFontSans = useCallback((id: string) => {
    setState((s) => ({ ...s, overrides: { ...s.overrides, fontSans: id } }));
  }, []);
  const setFontHeading = useCallback((id: string) => {
    setState((s) => ({
      ...s,
      overrides: { ...s.overrides, fontHeading: id },
    }));
  }, []);
  const setFontMono = useCallback((id: string) => {
    setState((s) => ({ ...s, overrides: { ...s.overrides, fontMono: id } }));
  }, []);

  const reset = useCallback(() => {
    setState((s) => ({ ...s, overrides: emptyOverrides() }));
  }, []);

  const loadProfile = useCallback(async (id: string) => {
    const profile = await getThemeProfile(id);
    if (!profile) throw new Error("Profile not found");
    const parsed = JSON.parse(profile.data) as ProfileData;
    setState({
      mode: parsed.mode ?? "system",
      overrides: {
        ...emptyOverrides(),
        ...(parsed.overrides ?? {}),
        colors: {
          light: parsed.overrides?.colors?.light ?? {},
          dark: parsed.overrides?.colors?.dark ?? {},
        },
      },
    });
    setActiveProfileId(id);
    setActiveProfileSnapshot(profile.data);
  }, []);

  const saveAsNewProfile = useCallback(
    async (name: string) => {
      const data = serializeProfile(state);
      const profile = await createThemeProfile(name, data);
      setActiveProfileId(profile.id);
      setActiveProfileSnapshot(profile.data);
      await refreshProfiles();
    },
    [state, refreshProfiles],
  );

  const saveActiveProfile = useCallback(async () => {
    if (!activeProfileId) throw new Error("No active profile");
    const current = profiles.find((p) => p.id === activeProfileId);
    const name = current?.name ?? "Untitled";
    const data = serializeProfile(state);
    const updated = await updateThemeProfile(activeProfileId, name, data);
    setActiveProfileSnapshot(updated.data);
    await refreshProfiles();
  }, [activeProfileId, profiles, state, refreshProfiles]);

  const renameActiveProfile = useCallback(
    async (name: string) => {
      if (!activeProfileId) throw new Error("No active profile");
      const data = serializeProfile(state);
      const updated = await updateThemeProfile(activeProfileId, name, data);
      setActiveProfileSnapshot(updated.data);
      await refreshProfiles();
    },
    [activeProfileId, state, refreshProfiles],
  );

  const deleteProfile = useCallback(
    async (id: string) => {
      await deleteThemeProfile(id);
      if (id === activeProfileId) {
        setActiveProfileId(null);
        setActiveProfileSnapshot(null);
      }
      await refreshProfiles();
    },
    [activeProfileId, refreshProfiles],
  );

  const clearActiveProfile = useCallback(() => {
    setActiveProfileId(null);
    setActiveProfileSnapshot(null);
  }, []);

  const value = useMemo<ThemeContextValue>(
    () => ({
      mode: state.mode,
      activeMode,
      overrides: state.overrides,
      setMode,
      setColor,
      clearColor,
      setRadius,
      setFontSans,
      setFontHeading,
      setFontMono,
      reset,
      profiles,
      activeProfileId,
      isDirty,
      refreshProfiles,
      loadProfile,
      saveAsNewProfile,
      saveActiveProfile,
      renameActiveProfile,
      deleteProfile,
      clearActiveProfile,
    }),
    [
      state.mode,
      activeMode,
      state.overrides,
      setMode,
      setColor,
      clearColor,
      setRadius,
      setFontSans,
      setFontHeading,
      setFontMono,
      reset,
      profiles,
      activeProfileId,
      isDirty,
      refreshProfiles,
      loadProfile,
      saveAsNewProfile,
      saveActiveProfile,
      renameActiveProfile,
      deleteProfile,
      clearActiveProfile,
    ],
  );

  return (
    <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>
  );
}

export function useTheme(): ThemeContextValue {
  const ctx = useContext(ThemeContext);
  if (!ctx) throw new Error("useTheme must be used within a ThemeProvider");
  return ctx;
}
