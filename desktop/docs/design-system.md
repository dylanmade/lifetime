# Lifetime Design System & UI Style Guide

The canonical rulebook for building UI in `desktop/`. Every interface change should
flow through this document. It is **living** — add to it whenever a new pattern,
token, or lesson emerges. Keep entries concrete and grounded in real files.

The foundation is **shadcn/ui (Radix-rhea preset)** with a custom theme. We own the
component code (it lives in `src/components/ui/`), the theme lives in `src/index.css`
as oklch CSS variables, and a live in-app editor (`Appearance.tsx`) writes those
variables at runtime. Two consequences drive almost every rule below:

1. **Everything visual is a token.** A hardcoded color or radius silently opts out
   of theming — the user's Appearance edits won't touch it. Reach for a token.
2. **The primitive probably already exists.** Before hand-rolling, check
   `src/components/ui/` and the shadcn registry. We've been burned hand-rolling
   things shadcn already ships.

---

## 1. Decision order: never hand-roll before checking

When a UI need arises, walk this list top to bottom and stop at the first hit:

1. **Already-installed primitive** — look in `src/components/ui/`. We currently have:
   `alert, badge, button, calendar, card, checkbox, combobox, command, dialog,
input, input-group, label, popover, select, separator, sheet, sidebar, skeleton,
slider, tabs, textarea, tooltip`.
2. **A primitive shadcn ships but we haven't added** — `npx shadcn@latest add <name>`
   (use `--dry-run` to inspect first). E.g. `collapsible`, `accordion`, `spinner`,
   `dropdown-menu`, `switch`, `radio-group`, `toggle-group`, `scroll-area`.
3. **A supported extension hook on a primitive** — Base UI primitives expose
   `render`, `asChild`, `filteredItems`, `virtualized`, etc. Prefer plugging into
   these over building infrastructure around the primitive.
4. **A project reusable component** — see §5. If the same composed shape recurs
   (≥3 uses), promote it to a named component.
5. **Only then, a bespoke element** — and when you do, say so explicitly in the PR/
   message and keep it token-driven.

> **Why this is rule #1:** we hand-rolled a font picker on Popover + cmdk, then again
> on Popover + manual virtualization, when a primitive was one `add` away — each
> version shipped subtle focus/ARIA/dismiss bugs the official primitive handles for
> free. Pattern-match the request to a component _name_ before designing anything.

**Picker decision sub-rule** (catalogs/selectors): small fixed options → `Select`;
the trigger itself is a typeable search field AND catalog <500 items → `Combobox`;
everything else (large/unknown catalogs) → **Mapo pattern**: `Button → Dialog →
Input + virtualized list` (reference: `src/theme/FontPicker.tsx`). Don't default to
Combobox for unbounded catalogs.

---

## 2. The token system (use these, not raw values)

### Color — never write a hex/oklch/hsl literal in a component

Use the semantic Tailwind color utilities backed by CSS variables. The full set
(light + `.dark`) is defined in `src/index.css` and exposed by the Appearance editor:

| Role      | Tokens                                                                            |
| --------- | --------------------------------------------------------------------------------- |
| Surfaces  | `background` `foreground` `card` `card-foreground` `popover` `popover-foreground` |
| Muted     | `muted` `muted-foreground`                                                        |
| Brand     | `primary` `primary-foreground` `accent` `accent-foreground`                       |
| Secondary | `secondary` `secondary-foreground`                                                |
| Feedback  | `destructive`                                                                     |
| Chrome    | `border` `input` `ring`                                                           |
| Sidebar   | `sidebar*` (8 tokens)                                                             |
| Data viz  | `chart-1` … `chart-5`                                                             |

Apply as `bg-card`, `text-muted-foreground`, `border-border`, `ring-ring`, etc.
Opacity modifiers are fine and encouraged (`bg-primary/70`, `bg-muted/30`).

**Exception — algorithmic data-viz color:** the timeline hashes app names to HSL
hues (`hueForApp` in `Timeline.tsx`) because the set of apps is unbounded and the
5 chart tokens can't cover it. This is an _intentional, documented_ exception. If a
viz has ≤5 stable series, use `chart-1..5` instead of inventing colors.

### Radius — driven by one `--radius` variable

`--radius` (default `0.625rem`) is user-editable; the scale derives from it:
`rounded-sm/md/lg/xl/2xl/3xl/4xl`. Cards use `rounded-[min(var(--radius-4xl),24px)]`.
Use the scale utilities. Don't write `rounded-[7px]`.

### Typography — three font tokens + a type scale

Fonts are tokens too (user-editable): `font-sans` (body), `font-heading`, `font-mono`.
Apply `font-heading` to titles, `font-mono` for code/timestamps/numeric IDs.

Type scale — stay on it:

| Use                                   | Classes                                                             |
| ------------------------------------- | ------------------------------------------------------------------- |
| Page title (`<h1>`)                   | `text-2xl font-semibold tracking-tight`                             |
| Card / section title                  | via `CardTitle` (`font-heading text-base font-medium`)              |
| **Section eyebrow** (uppercase label) | see §5 `SectionLabel`                                               |
| Body                                  | `text-sm` (this is our default density — most UI text is `text-sm`) |
| Secondary / hint                      | `text-muted-foreground text-sm`                                     |
| Meta / caption                        | `text-xs`                                                           |
| Numeric columns                       | add `tabular-nums`; timestamps add `font-mono`                      |

Avoid arbitrary `text-[10px]`/`text-[11px]`. If you genuinely need a sub-`text-xs`
size for dense chart axis labels, that's the one sanctioned use — keep it to
data-viz tick/label text, not chrome.

### Spacing — a fixed rhythm, applied consistently

This is where drift shows up most. Use these and only these for each context:

| Context                                  | Spacing                                   |
| ---------------------------------------- | ----------------------------------------- |
| Page root vertical stack                 | `space-y-6`                               |
| Multi-card grid gap                      | `gap-6`                                   |
| Inside a card: content blocks            | `space-y-4` (forms) / `space-y-3` (lists) |
| Form field group (label + control)       | `space-y-1.5`                             |
| Row of buttons                           | `gap-2`                                   |
| Tight icon-button cluster (e.g. day nav) | `gap-1`                                   |
| Icon + text inside a heading/label       | `gap-2`                                   |
| List item internal label/value           | `gap-3` / `gap-4`                         |

Card internal padding (`px-5`/`py-5`, `sm` → 4) is owned by the `Card` primitive —
**do not** add your own padding to `CardContent`; change the card `size` instead.

---

## 3. Page layout contract

Every routed page renders inside `App.tsx`'s `<main className="flex-1 p-6"><div
className="mx-auto w-full max-w-6xl">`. So a page component should:

- Start with `<div className="space-y-6">` as its root.
- Lead with an `<h1 className="text-2xl font-semibold tracking-tight">`.
- Optionally a one-line `<p className="text-muted-foreground text-sm">` subtitle.
- Group content into `Card`s; use `grid gap-6 lg:grid-cols-2` for multi-column.

Don't add page-level horizontal padding or max-width — the shell owns that.

---

## 4. Buttons & icons (high-drift area — read this)

- Use the `Button` component for anything clickable that looks like a button.
  Never hand-roll a `<button>` with bespoke focus rings unless it's a genuinely
  non-button element (a color swatch, a virtualized list row).
- **Icons in buttons need no margin and no size class.** `Button` already applies
  `gap-1.5` and auto-sizes SVGs to `size-4`. Write `<Button><Plus />Label</Button>`,
  **not** `<Button><Plus className="mr-2 h-4 w-4" />Label</Button>`. The `mr-2` both
  double-spaces and diverges from buttons that rely on the gap.
- Icon-only buttons: `size="icon"` (or `icon-sm`/`icon-lg`) — don't fake it with
  padding on a default button.
- Standalone icons (outside buttons): standardize on `size-4` (not `h-4 w-4`).
  Use `size-3.5` only for deliberately smaller affordances.
- Destructive actions: `variant="destructive"`. Don't bolt `text-destructive` onto
  an `outline` button except for a low-emphasis destructive action in a toolbar
  (e.g. ProfileSection's Delete), and prefer the real variant when it's the primary
  action in a confirm dialog.

---

## 5. When to make a reusable component

Promote to a named, reusable component when **the same composed shape appears ≥3
times** or when it encodes a decision we don't want re-derived each time.

**"Usage" is not limited to shared _functionality_ — shared _appearance_ counts just
as much.** A component can exist purely to guarantee visual consistency across
surfaces that don't otherwise behave alike. `Card` is exactly this: it was made
reusable so every card-like surface in the app looks identical (same padding, radius,
ring, shadow), regardless of what each card _does_. So when you see the same visual
treatment recurring — even across functionally unrelated spots — that's a reuse
signal, not a coincidence to copy-paste. The eyebrow `SectionLabel` below is the same
idea: one look, many unrelated sections.

Existing shared components built on this principle (in `src/components/`):

- **`SectionLabel`** (`section-label.tsx`) — the uppercase eyebrow title. Pure
  visual-consistency component (used 8× across unrelated sections). Use it for any
  muted/tracked/uppercase section heading, including inside a `CardHeader` or a
  collapsible trigger.
- **`DayNavHeader`** (`day-nav-header.tsx`) — the `h1 + ‹ calendar ›` date navigator
  shared by `Summary` and `Timeline`. Takes `selectedDate` + an `onSelectDate`
  callback so each page applies its own normalization/side effects.

Keep reusable components **token-driven** and put shared primitives in
`src/components/` (design-system primitives and feature-shared components) or
`src/components/ui/` (vendored shadcn primitives). A reusable component that
hardcodes spacing/color defeats the purpose.

Don't over-abstract: a shape used twice with diverging needs can stay inline. The
bar is recurrence _plus_ stability.

---

## 6. Behavior conventions

- **Theme changes apply live** — wire theme controls' `onChange` straight to a CSS
  variable write + localStorage; no Save button in the editor. Named profiles are a
  separate explicit save.
- **Virtualize catalogs >~200 items** from day one (`@tanstack/react-virtual`),
  integrated through the primitive's `virtualized` prop where one exists. Give the
  scroll container a literal pixel height measurable at first paint.
- **Tailwind class order** — trust `prettier-plugin-tailwindcss`; don't hand-sort.
- **`useEffect`** — consult the "You Might Not Need an Effect" principles before
  adding one; event-driven logic belongs in handlers (see `Timeline.tsx` `selectDay`).

---

## 7. How to extend this guide

Add a subsection under the relevant numbered section, or a new top-level section.
Keep the format: a **rule**, a one-line **why** (ideally citing the incident or file
that motivated it), and concrete **how to apply** guidance. Prune rules that stop
being true.
</invoke>
