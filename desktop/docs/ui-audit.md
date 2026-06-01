# UI Audit — against `design-system.md`

> **Status (2026-05-31): all findings below resolved in the same pass.** Added
> `SectionLabel` + `DayNavHeader` (`src/components/`), pulled in shadcn `collapsible`
> + `spinner`, stripped redundant button-icon margins/sizes, standardized standalone
> icons on `size-*`. Kept the two borderline `text-[10px]` data-viz labels (#4) as
> deliberate. `tsc` + `vite build` clean. This document is retained as the audit
> method/checklist for future passes.

Snapshot date: 2026-05-31. Audited every file under `desktop/src/` that renders UI.
Overall: **the codebase is in good shape** — consistent page scaffolding, disciplined
token use for color/radius, shadcn-first almost everywhere. Findings below are mostly
small consistency drifts plus two "should've used a primitive / should be reusable"
items. Severity: 🔴 fix soon · 🟡 worth doing · 🟢 minor/nice-to-have.

---

## 🔴 1. Hand-rolled collapsible instead of the shadcn primitive

`Appearance.tsx` `ColorSection` builds a disclosure from a raw `<button>` + a rotating
`ChevronDown` + conditional render of the body. This is exactly the decision-order
violation §1 warns about — shadcn ships **`Collapsible`** (and `Accordion`).

- **Where:** `src/Appearance.tsx:144-170`
- **Fix:** `npx shadcn@latest add collapsible`, replace the manual open-state +
  button + conditional body. Bonus: `Accordion` would give single-open-at-a-time
  behavior across the color groups for free if that's desired.
- **Why it matters:** the hand-rolled version reimplements `aria-expanded`, focus,
  and animation that the primitive handles; it's the same class of bug-surface as the
  font-picker incident.

## 🔴 2. Eyebrow section title duplicated 8× — should be one component

The uppercase label `text-muted-foreground text-xs font-medium tracking-wider uppercase`
is copy-pasted as a `CardTitle` className across the app. Meets the §5 reuse bar easily.

- **Where:** `Summary.tsx:178,216,247` · `Appearance.tsx:61,83,115,154` ·
  `ProfileSection.tsx:47` · `Timeline.tsx:436`
- **Fix:** introduce `SectionLabel` (or a `CardTitle` `variant="eyebrow"`). One source
  of truth means the next tweak (letter-spacing, color) is a one-line change instead
  of a find-replace.

---

## 🟡 3. Redundant `mr-2` / `h-4 w-4` on button icons (spacing drift)

`Button` already applies `gap-1.5` and auto-sizes SVGs to `size-4`, so icons inside
buttons need neither. Some call sites add `mr-2 h-4 w-4` (→ 8px gap), others rely on
the built-in gap (→ 6px). **This is the inconsistent padding you noticed** — same
button type, two different icon gaps.

- **With `mr-2` (should be removed):** `Appearance.tsx:52,73` · `Summary.tsx:119` ·
  `Timeline.tsx:389` · `ProfileSection.tsx:95,104,114,123`
- **Already correct (relies on gap):** `Summary.tsx:285` (`<Plus className="h-4 w-4" />`
  — still has the redundant size class, but no margin)
- **Fix:** drop `mr-2` everywhere inside `Button`; drop `h-4 w-4` too and let the
  button size the SVG. `FontPicker.tsx:126`'s `ml-2` on the trailing chevron is the
  one defensible case (pushing a right-aligned icon in a `justify-between` button),
  but `gap` would also cover it.

## 🟡 4. Arbitrary `text-[10px]` outside data-viz

§2 sanctions sub-`text-xs` only for dense chart tick/axis labels. Two of the four uses
qualify; check the others.

- **Sanctioned (chart/tick labels):** `Summary.tsx:196` (hour-axis), `Timeline.tsx:509`
  (tick labels)
- **Borderline — review:** `Timeline.tsx:315` (activity-block label text — it's a
  content label, not an axis; consider `text-xs`), `FontPicker.tsx:229` (category tag
  in list rows — consider `text-xs` or accept as a deliberate caption).

## 🟡 5. `DayNavHeader` duplicated between Summary and Timeline

The `h1 + previous/calendar-popover/next` date navigator is ~45 near-identical lines
in both files. Meets the §5 reuse bar.

- **Where:** `Summary.tsx:103-153` vs `Timeline.tsx:373-421`
- **Fix:** extract `<DayNavHeader date selectedDate onSelect maxDate />`. They differ
  only in `onSelect` (Timeline also resets scroll via `selectDay`) — a callback prop
  covers it.

---

## 🟢 6. Hand-rolled spinner

`EnableEncryption.tsx:150` builds a spinner from
`border-primary ... animate-spin rounded-full border-2 border-t-transparent`. shadcn
now ships **`Spinner`** (`npx shadcn@latest add spinner`). Low traffic, but it's a
primitive we're reimplementing.

## 🟢 7. `size-4` vs `h-4 w-4` convention drift

`AppSidebar.tsx:44-45` uses `size-4`/`size-8`; everywhere else standalone icons use
`h-4 w-4`. §4 standardizes on `size-*`. Purely cosmetic in output; pick one for
grep-ability. (Recommend `size-*`, matching shadcn's own components.)

## 🟢 8. Minor vertical-rhythm drift in Appearance rows

`ColorInput` row uses `py-1` and the Radius row uses `py-1`, but `FontRow`
(`Appearance.tsx:172`) has no vertical padding — the typography rows sit slightly
tighter than the color/shape rows. Add `py-1` to `FontRow` for consistent row height,
or drop it everywhere and rely on the container's `space-y-4`.

## 🟢 9. Hand-rolled focus ring on the color swatch

`ColorInput.tsx:53-58`'s swatch `<button>` is a legitimately bespoke element (a color
chip, not a Button), but it hand-writes `ring-offset-background focus-visible:ring-ring
focus-visible:ring-2 ...`. Fine to keep; just noting it duplicates focus-ring styling
that the Button variant centralizes. If a second swatch-like control appears, factor
the ring into a shared class.

---

## Not findings (verified clean)

- **Color tokens:** no stray hex/oklch literals in components. The only non-token
  color is `Timeline.tsx`'s `hueForApp` HSL hashing — a sanctioned data-viz exception
  (§2).
- **Radius:** all via the `rounded-*` scale or the card's `min(var(--radius-4xl),24px)`.
- **Page scaffolding:** `Summary`, `Timeline`, `Settings`, `Appearance` all follow the
  §3 contract (`space-y-6` root, `text-2xl font-semibold tracking-tight` h1).
- **Form field groups:** consistently `space-y-1.5` (label+control) inside `space-y-4`
  forms across `LogActivity`, `UnlockModal`, `EnableEncryption`.
- **Pickers:** `FontPicker` correctly uses the Mapo pattern (large catalog), profile
  selector correctly uses `Select` (small fixed set).
- **`App.css`:** already emptied; safe to delete once confirmed nothing imports it.

---

## Suggested order of operations

1. (🟡 #3) Strip redundant `mr-2`/sizes from button icons — pure mechanical, removes
   the visible spacing inconsistency.
2. (🔴 #2) Add `SectionLabel`, swap the 8 call sites.
3. (🔴 #1) Replace `ColorSection` with `Collapsible`.
4. (🟡 #5) Extract `DayNavHeader`.
5. (🟢) The rest as cleanup.
