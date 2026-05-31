// All shadcn color tokens that the editor exposes. Order roughly follows the
// :root block in index.css. The id matches the CSS custom property name
// (without the leading `--`).
export type ColorToken =
  | "background"
  | "foreground"
  | "card"
  | "card-foreground"
  | "popover"
  | "popover-foreground"
  | "primary"
  | "primary-foreground"
  | "secondary"
  | "secondary-foreground"
  | "muted"
  | "muted-foreground"
  | "accent"
  | "accent-foreground"
  | "destructive"
  | "border"
  | "input"
  | "ring"
  | "chart-1"
  | "chart-2"
  | "chart-3"
  | "chart-4"
  | "chart-5"
  | "sidebar"
  | "sidebar-foreground"
  | "sidebar-primary"
  | "sidebar-primary-foreground"
  | "sidebar-accent"
  | "sidebar-accent-foreground"
  | "sidebar-border"
  | "sidebar-ring";

export type ColorGroup = {
  name: string;
  defaultOpen: boolean;
  tokens: { id: ColorToken; label: string }[];
};

export const COLOR_GROUPS: ColorGroup[] = [
  {
    name: "Surface",
    defaultOpen: true,
    tokens: [
      { id: "background", label: "Background" },
      { id: "foreground", label: "Foreground" },
      { id: "card", label: "Card" },
      { id: "card-foreground", label: "Card foreground" },
      { id: "popover", label: "Popover" },
      { id: "popover-foreground", label: "Popover foreground" },
      { id: "muted", label: "Muted" },
      { id: "muted-foreground", label: "Muted foreground" },
    ],
  },
  {
    name: "Brand",
    defaultOpen: true,
    tokens: [
      { id: "primary", label: "Primary" },
      { id: "primary-foreground", label: "Primary foreground" },
      { id: "accent", label: "Accent" },
      { id: "accent-foreground", label: "Accent foreground" },
    ],
  },
  {
    name: "Secondary",
    defaultOpen: false,
    tokens: [
      { id: "secondary", label: "Secondary" },
      { id: "secondary-foreground", label: "Secondary foreground" },
    ],
  },
  {
    name: "Feedback",
    defaultOpen: false,
    tokens: [{ id: "destructive", label: "Destructive" }],
  },
  {
    name: "UI",
    defaultOpen: false,
    tokens: [
      { id: "border", label: "Border" },
      { id: "input", label: "Input" },
      { id: "ring", label: "Ring" },
    ],
  },
  {
    name: "Sidebar",
    defaultOpen: false,
    tokens: [
      { id: "sidebar", label: "Background" },
      { id: "sidebar-foreground", label: "Foreground" },
      { id: "sidebar-primary", label: "Primary" },
      { id: "sidebar-primary-foreground", label: "Primary foreground" },
      { id: "sidebar-accent", label: "Accent" },
      { id: "sidebar-accent-foreground", label: "Accent foreground" },
      { id: "sidebar-border", label: "Border" },
      { id: "sidebar-ring", label: "Ring" },
    ],
  },
  {
    name: "Chart",
    defaultOpen: false,
    tokens: [
      { id: "chart-1", label: "Chart 1" },
      { id: "chart-2", label: "Chart 2" },
      { id: "chart-3", label: "Chart 3" },
      { id: "chart-4", label: "Chart 4" },
      { id: "chart-5", label: "Chart 5" },
    ],
  },
];

export type FontOption = {
  id: string;
  label: string;
  stack: string;
};

export const SANS_FONTS: FontOption[] = [
  { id: "dm-sans", label: "DM Sans", stack: `"DM Sans Variable", sans-serif` },
  { id: "inter", label: "Inter", stack: `"Inter Variable", sans-serif` },
  { id: "geist", label: "Geist", stack: `"Geist Variable", sans-serif` },
  {
    id: "system",
    label: "System",
    stack: `system-ui, -apple-system, "Segoe UI", sans-serif`,
  },
];

export const MONO_FONTS: FontOption[] = [
  {
    id: "jetbrains-mono",
    label: "JetBrains Mono",
    stack: `"JetBrains Mono Variable", monospace`,
  },
  {
    id: "system-mono",
    label: "System mono",
    stack: `ui-monospace, "SF Mono", Menlo, monospace`,
  },
];

export const DEFAULT_RADIUS_REM = 0.625;
export const MIN_RADIUS_REM = 0;
export const MAX_RADIUS_REM = 1.5;
