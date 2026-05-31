import { formatHex, parse } from "culori";

// Convert any CSS color (oklch, rgb, hex, named) to a #rrggbb hex string.
// Returns "#000000" on parse failure rather than throwing — the picker UI
// needs *some* starting color and there's no good fallback for a malformed
// token value.
export function toHex(cssColor: string): string {
  const trimmed = cssColor.trim();
  if (!trimmed) return "#000000";
  const parsed = parse(trimmed);
  if (!parsed) return "#000000";
  const hex = formatHex(parsed);
  return hex ?? "#000000";
}

// Read the currently-applied value of a CSS custom property on :root and
// convert it to hex. Used when opening the picker so it starts on the color
// currently showing in the UI.
export function readTokenHex(tokenId: string): string {
  if (typeof document === "undefined") return "#000000";
  const raw = getComputedStyle(document.documentElement)
    .getPropertyValue(`--${tokenId}`)
    .trim();
  return toHex(raw);
}
