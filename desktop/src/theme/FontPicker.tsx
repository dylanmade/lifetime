import { useCallback, useEffect, useMemo, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { Check, ChevronDown, XIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/utils";
import type { FontOption } from "./tokens";
import {
  GOOGLE_FONTS,
  categoryFallback,
  googleFontFamilyFromId,
  googleFontIdFor,
  isGoogleFontId,
  loadGoogleFont,
} from "./googleFonts";

type FontItem = {
  id: string;
  label: string;
  labelLower: string;
  stack: string;
  category?: string;
  googleFamily?: string;
};

type Props = {
  value: string;
  bundled: FontOption[];
  // Restrict Google Fonts list to Monospace category (for mono pickers).
  monoOnly?: boolean;
  onChange: (id: string) => void;
};

const ROW_HEIGHT = 44;
const LIST_HEIGHT = 400;

export function FontPicker({ value, bundled, monoOnly, onChange }: Props) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  // Callback ref via useState so attaching the scroll element triggers a
  // re-render — the virtualizer reads getScrollElement on every render but
  // doesn't subscribe to a useRef changing.
  const [scrollEl, setScrollEl] = useState<HTMLDivElement | null>(null);

  const items = useMemo<FontItem[]>(() => {
    const bundledItems: FontItem[] = bundled.map((f) => ({
      id: f.id,
      label: f.label,
      labelLower: f.label.toLowerCase(),
      stack: f.stack,
    }));
    const googleSource = monoOnly
      ? GOOGLE_FONTS.filter((g) => g.c === "Monospace")
      : GOOGLE_FONTS;
    const googleItems: FontItem[] = googleSource.map((g) => ({
      id: googleFontIdFor(g.f),
      label: g.f,
      labelLower: g.f.toLowerCase(),
      stack: `"${g.f}", ${categoryFallback(g.c)}`,
      category: g.c,
      googleFamily: g.f,
    }));
    return [...bundledItems, ...googleItems];
  }, [bundled, monoOnly]);

  const selectedItem = useMemo(
    () => items.find((i) => i.id === value),
    [items, value],
  );

  const filteredItems = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return items;
    return items.filter((item) => item.labelLower.includes(q));
  }, [items, query]);

  const virtualizer = useVirtualizer({
    count: filteredItems.length,
    getScrollElement: () => scrollEl,
    estimateSize: () => ROW_HEIGHT,
    overscan: 6,
  });

  const handleOpenChange = useCallback((nextOpen: boolean) => {
    setOpen(nextOpen);
    if (!nextOpen) setQuery("");
  }, []);

  const select = useCallback(
    (item: FontItem) => {
      onChange(item.id);
      setOpen(false);
      setQuery("");
    },
    [onChange],
  );

  // Load the currently-selected font so the trigger button renders its label
  // in the chosen face.
  useEffect(() => {
    if (selectedItem?.googleFamily) loadGoogleFont(selectedItem.googleFamily);
  }, [selectedItem?.googleFamily]);

  const triggerLabel =
    selectedItem?.label ??
    (isGoogleFontId(value)
      ? (googleFontFamilyFromId(value) ?? "Select font")
      : "Select font");

  return (
    <>
      <Button
        variant="outline"
        onClick={() => setOpen(true)}
        className="w-64 justify-between font-normal"
      >
        <span className="truncate" style={{ fontFamily: selectedItem?.stack }}>
          {triggerLabel}
        </span>
        <ChevronDown className="text-muted-foreground size-4 shrink-0" />
      </Button>
      <Dialog open={open} onOpenChange={handleOpenChange}>
        <DialogContent
          showCloseButton={false}
          className="gap-0 p-0 sm:max-w-md"
        >
          {/* Content is full-bleed (p-0) so the virtualized list and its
              border-t dividers span edge to edge. The header + search live in
              one padded block (p-4, space-y-3) so they aren't crowded. The
              default close button is absolutely positioned for a p-6 dialog and
              sits low against this tighter header, so we disable it and render
              the close button inline in the header row, centered with the
              title. */}
          <div className="space-y-3 p-4">
            <DialogHeader className="flex-row items-center justify-between">
              <DialogTitle>Choose font</DialogTitle>
              <DialogClose asChild>
                <Button
                  variant="ghost"
                  size="icon-sm"
                  className="bg-secondary -my-1"
                  aria-label="Close"
                >
                  <XIcon />
                </Button>
              </DialogClose>
            </DialogHeader>
            <Input
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Search fonts…"
              autoFocus
              spellCheck={false}
              autoComplete="off"
            />
          </div>
          {filteredItems.length === 0 ? (
            <div className="text-muted-foreground border-t p-4 text-center text-sm">
              No fonts found.
            </div>
          ) : (
            <div
              ref={setScrollEl}
              className="border-t"
              style={{ height: LIST_HEIGHT, overflow: "auto" }}
            >
              <div
                style={{
                  height: virtualizer.getTotalSize(),
                  position: "relative",
                  width: "100%",
                }}
              >
                {virtualizer.getVirtualItems().map((vi) => {
                  const item = filteredItems[vi.index];
                  if (!item) return null;
                  return (
                    <div
                      key={item.id}
                      style={{
                        position: "absolute",
                        top: 0,
                        left: 0,
                        width: "100%",
                        height: vi.size,
                        transform: `translateY(${vi.start}px)`,
                      }}
                    >
                      <FontItemRow
                        item={item}
                        selected={value === item.id}
                        onSelect={() => select(item)}
                      />
                    </div>
                  );
                })}
              </div>
            </div>
          )}
        </DialogContent>
      </Dialog>
    </>
  );
}

function FontItemRow({
  item,
  selected,
  onSelect,
}: {
  item: FontItem;
  selected: boolean;
  onSelect: () => void;
}) {
  // Lazy-load the Google Font when this row mounts (virtualizer window).
  // Idempotent — the loader dedupes via an internal Set.
  useEffect(() => {
    if (item.googleFamily) loadGoogleFont(item.googleFamily);
  }, [item.googleFamily]);

  return (
    <button
      type="button"
      onClick={onSelect}
      className={cn(
        "hover:bg-accent flex h-full w-full items-center gap-3 px-4 text-left transition-colors",
        selected && "bg-accent/40",
      )}
    >
      <Check
        className={cn(
          "size-4 shrink-0",
          selected ? "opacity-100" : "opacity-0",
        )}
      />
      <span
        className="flex-1 truncate text-base"
        style={{ fontFamily: item.stack }}
      >
        {item.label}
      </span>
      {item.category && (
        <span className="text-muted-foreground shrink-0 text-[10px] tracking-wider uppercase">
          {item.category}
        </span>
      )}
    </button>
  );
}
