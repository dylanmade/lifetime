import catalogJson from "./googleFonts.json";

export type GoogleFontCategory =
  | "Sans Serif"
  | "Serif"
  | "Display"
  | "Handwriting"
  | "Monospace";

export type GoogleFontEntry = { f: string; c: GoogleFontCategory };

export const GOOGLE_FONTS: GoogleFontEntry[] = catalogJson as GoogleFontEntry[];

const GOOGLE_PREFIX = "g:";

export function googleFontIdFor(family: string): string {
  return `${GOOGLE_PREFIX}${family}`;
}

export function isGoogleFontId(id: string | undefined): boolean {
  return !!id && id.startsWith(GOOGLE_PREFIX);
}

export function googleFontFamilyFromId(id: string): string | null {
  return id.startsWith(GOOGLE_PREFIX) ? id.slice(GOOGLE_PREFIX.length) : null;
}

export function categoryFallback(category: GoogleFontCategory): string {
  switch (category) {
    case "Serif":
      return "serif";
    case "Monospace":
      return "monospace";
    case "Handwriting":
      return "cursive";
    case "Display":
    case "Sans Serif":
    default:
      return "sans-serif";
  }
}

// Convert a Google Font family name into a CSS font stack like
// `"Inter", sans-serif`, picking a generic fallback by category.
//
// Performance: scans the full catalog (1,900+) to find the entry. Callers
// that already have the catalog entry should build the stack inline via
// `categoryFallback(entry.c)` — see FontCombobox — to avoid O(N²) when
// iterating the catalog.
export function googleFontStack(family: string): string {
  const entry = GOOGLE_FONTS.find((g) => g.f === family);
  const fallback = entry ? categoryFallback(entry.c) : "sans-serif";
  return `"${family}", ${fallback}`;
}

// Track which families we've already injected so we don't pile up duplicate
// <link> tags as the user scrubs through the picker.
const loadedFonts = new Set<string>();

// Inject a <link rel="stylesheet"> pointing at the Google Fonts CSS2 API for
// the given family. The browser caches subsequent loads. Idempotent.
//
// Privacy note: this sends an HTTPS request to fonts.googleapis.com when a
// Google Font is first selected. Users who want strict offline behavior can
// stick to the bundled fonts (DM Sans, Inter, Geist, JetBrains Mono).
export function loadGoogleFont(family: string): void {
  if (typeof document === "undefined") return;
  if (loadedFonts.has(family)) return;
  loadedFonts.add(family);

  // Request common UI weights. Google Fonts substitutes the closest available
  // weight when a family doesn't have one of these exactly.
  const familyParam = encodeURIComponent(family).replace(/%20/g, "+");
  const url = `https://fonts.googleapis.com/css2?family=${familyParam}:wght@400;500;600;700&display=swap`;

  const link = document.createElement("link");
  link.rel = "stylesheet";
  link.href = url;
  link.dataset.lifetimeGoogleFont = family;
  document.head.appendChild(link);
}
