import { useEffect, useMemo, useRef, useState } from "react";
import { ChevronFirst, ChevronLast, Minus, Plus } from "lucide-react";
import { Card, CardContent, CardHeader } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { SectionLabel } from "@/components/section-label";
import { DayNavHeader } from "@/components/day-nav-header";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { type ResolvedActivity, getActivitiesBetween } from "./api";
import { ActivityDetail } from "./ActivityDetail";
import { LogActivity } from "./LogActivity";
import {
  DeviceScopeToggle,
  type DeviceScope,
  scopeArg,
} from "@/components/device-scope-toggle";
import {
  addDays,
  formatDuration,
  formatTime,
  isSameDay,
  startOfDay,
} from "@/lib/datetime";

const POLL_INTERVAL_MS = 10_000;
const DAY_SECONDS = 24 * 3600;

const MIN_ZOOM = 1;
const MAX_ZOOM = 96; // 15-minute window
const BUTTON_ZOOM_FACTOR = 1.5;

// Quick-zoom presets, in hours of visible window. zoom = 24 / windowHours.
const QUICK_WINDOW_HOURS = [1, 3, 6, 12, 24];

// Hover-to-add ghost on the manual lane: a snapped empty slot under the cursor.
type GhostSpan = {
  leftPct: number;
  widthPct: number;
  start: Date;
  end: Date;
};

function pad2(n: number): string {
  return String(n).padStart(2, "0");
}

function clamp(value: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, value));
}

// Stable HSL hue per app so each app keeps the same color across days.
function hueForApp(appName: string): number {
  let hash = 0;
  for (let i = 0; i < appName.length; i++) {
    hash = (hash * 31 + appName.charCodeAt(i)) >>> 0;
  }
  return hash % 360;
}

function appColor(appName: string, isActive: boolean): string {
  const hue = hueForApp(appName);
  return isActive ? `hsl(${hue} 65% 55%)` : `hsl(${hue} 20% 45% / 0.35)`;
}

function positionInDay(
  startIso: string,
  endIso: string,
  dayStart: Date,
): { leftPct: number; widthPct: number } {
  const dayStartMs = dayStart.getTime();
  const dayMs = DAY_SECONDS * 1000;
  const startMs = new Date(startIso).getTime();
  const endMs = new Date(endIso).getTime();
  const leftPct = Math.max(0, ((startMs - dayStartMs) / dayMs) * 100);
  const widthPct = Math.max(0, ((endMs - startMs) / dayMs) * 100);
  return { leftPct, widthPct };
}

// Visible window in hours at a given zoom level.
function windowHours(zoom: number): number {
  return 24 / zoom;
}

function formatWindow(zoom: number): string {
  const hours = windowHours(zoom);
  if (hours >= 1) {
    return Number.isInteger(hours) ? `${hours}h` : `${hours.toFixed(1)}h`;
  }
  return `${Math.round(hours * 60)}m`;
}

// Choose tick spacing so labels stay legible as the user zooms in.
function tickIntervalHours(zoom: number): number {
  if (zoom < 2) return 3;
  if (zoom < 4) return 2;
  if (zoom < 8) return 1;
  if (zoom < 16) return 0.5;
  if (zoom < 48) return 0.25;
  return 1 / 12; // 5 min
}

function formatTick(hours: number): string {
  const h = Math.floor(hours);
  const m = Math.round((hours - h) * 60);
  if (m === 0) return pad2(h);
  return `${pad2(h)}:${pad2(m)}`;
}

export function Timeline() {
  const [selectedDate, setSelectedDate] = useState(() =>
    startOfDay(new Date()),
  );
  const [activities, setActivities] = useState<ResolvedActivity[]>([]);
  const [selected, setSelected] = useState<ResolvedActivity | null>(null);
  const [ghost, setGhost] = useState<GhostSpan | null>(null);
  const [addSpan, setAddSpan] = useState<{ start: Date; end: Date } | null>(
    null,
  );
  const [addOpen, setAddOpen] = useState(false);
  // Bumped on each open so the add dialog remounts with fresh pre-filled times.
  const [addKey, setAddKey] = useState(0);
  const [refreshTick, setRefreshTick] = useState(0);
  const [scope, setScope] = useState<DeviceScope>("local");
  const [error, setError] = useState<string | null>(null);
  const [now, setNow] = useState(() => new Date());
  const [zoom, setZoom] = useState(1);

  const cardRef = useRef<HTMLDivElement>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  const innerRef = useRef<HTMLDivElement>(null);
  const zoomRef = useRef(zoom);
  const pendingSyncRef = useRef(false);

  const isToday = isSameDay(selectedDate, new Date());

  useEffect(() => {
    let cancelled = false;

    async function fetchAll() {
      try {
        const dayEnd = addDays(selectedDate, 1);
        const end = isToday ? new Date() : dayEnd;
        const acts = await getActivitiesBetween(
          selectedDate.toISOString(),
          end.toISOString(),
          scopeArg(scope),
        );
        if (!cancelled) {
          setActivities(acts);
          setError(null);
        }
      } catch (e) {
        if (!cancelled) setError(String(e));
      }
    }

    fetchAll();

    if (!isToday) {
      return () => {
        cancelled = true;
      };
    }
    const id = setInterval(() => {
      fetchAll();
      setNow(new Date());
    }, POLL_INTERVAL_MS);
    return () => {
      cancelled = true;
      clearInterval(id);
    };
  }, [selectedDate, isToday, refreshTick, scope]);

  // Single entry point for changing the selected day. Resets scroll alongside
  // the state update so we don't carry over an unrelated viewport offset.
  // (Per React docs: this kind of "in response to an event" logic belongs in
  // the event handler, not an effect.)
  function selectDay(date: Date) {
    setSelectedDate(startOfDay(date));
    if (scrollRef.current) scrollRef.current.scrollLeft = 0;
  }

  // Apply a zoom factor while keeping the time under `focalScreenX` fixed in
  // place. Writes to the DOM directly (no React round-trip per gesture event)
  // so rapid wheel events compose correctly — each event reads the post-write
  // scrollWidth from the previous one. React state is synced once per frame
  // for derived UI (tick density, button label).
  function applyZoom(focalScreenX: number, newZoomRaw: number) {
    const container = scrollRef.current;
    const inner = innerRef.current;
    if (!container || !inner) return;

    const newZoom = clamp(newZoomRaw, MIN_ZOOM, MAX_ZOOM);
    if (newZoom === zoomRef.current) return;

    const oldScrollWidth = container.scrollWidth;
    if (oldScrollWidth === 0) return;
    const focalContentX = container.scrollLeft + focalScreenX;
    const focalRatio = focalContentX / oldScrollWidth;

    zoomRef.current = newZoom;
    inner.style.width = `${newZoom * 100}%`;

    // scrollWidth reflects the new layout synchronously after the style write.
    const newScrollWidth = container.scrollWidth;
    container.scrollLeft = focalRatio * newScrollWidth - focalScreenX;

    if (!pendingSyncRef.current) {
      pendingSyncRef.current = true;
      requestAnimationFrame(() => {
        pendingSyncRef.current = false;
        setZoom(zoomRef.current);
      });
    }
  }

  // Pinch-to-zoom over the entire Card. macOS trackpad pinch fires `wheel`
  // with `ctrlKey: true`. React's synthetic onWheel is passive — preventDefault
  // doesn't take — so we attach a native listener with { passive: false }.
  useEffect(() => {
    const card = cardRef.current;
    if (!card) return;

    function handleWheel(e: WheelEvent) {
      if (!e.ctrlKey) return;
      const container = scrollRef.current;
      if (!container) return;
      e.preventDefault();
      const rect = container.getBoundingClientRect();
      // Cursor may be over the header or outside the lanes; clamp the focal X
      // into the scroll container's range so it stays meaningful.
      const focalScreenX = clamp(
        e.clientX - rect.left,
        0,
        container.clientWidth,
      );
      const factor = Math.exp(-e.deltaY * 0.01);
      applyZoom(focalScreenX, zoomRef.current * factor);
    }

    card.addEventListener("wheel", handleWheel, { passive: false });
    return () => card.removeEventListener("wheel", handleWheel);
  }, []);

  function zoomFromButton(factor: number) {
    const container = scrollRef.current;
    if (!container) return;
    applyZoom(container.clientWidth / 2, zoomRef.current * factor);
  }

  // Jump to a preset window width, keeping the center of the current view fixed.
  function zoomToWindow(hours: number) {
    const container = scrollRef.current;
    if (!container) return;
    applyZoom(container.clientWidth / 2, 24 / hours);
  }

  // Pan the (zoomed) timeline to its far edges. No-op at zoom 1 since there's
  // no horizontal overflow — the buttons are disabled there.
  function jumpToStart() {
    scrollRef.current?.scrollTo({ left: 0, behavior: "smooth" });
  }

  function jumpToEnd() {
    const c = scrollRef.current;
    if (c) c.scrollTo({ left: c.scrollWidth, behavior: "smooth" });
  }

  function resetZoom() {
    const container = scrollRef.current;
    const inner = innerRef.current;
    if (!container || !inner) return;
    zoomRef.current = 1;
    inner.style.width = "100%";
    container.scrollLeft = 0;
    setZoom(1);
  }

  const nowPct = useMemo(() => {
    if (!isToday) return null;
    const dayMs = DAY_SECONDS * 1000;
    const offset = now.getTime() - selectedDate.getTime();
    if (offset < 0 || offset > dayMs) return null;
    return (offset / dayMs) * 100;
  }, [isToday, now, selectedDate]);

  const autoActivities = useMemo(
    () => activities.filter((a) => a.source === "auto"),
    [activities],
  );

  const totalActive = useMemo(
    () =>
      autoActivities
        .filter((a) => a.is_active)
        .reduce(
          (sum, a) =>
            sum +
            (new Date(a.ends_at!).getTime() - new Date(a.starts_at).getTime()) /
              1000,
          0,
        ),
    [autoActivities],
  );

  const ticks = useMemo(() => {
    const interval = tickIntervalHours(zoom);
    const result: number[] = [];
    for (let t = 0; t < 24; t += interval) {
      result.push(t);
    }
    result.push(24);
    return result;
  }, [zoom]);

  // Completed manual activities (those with an end) render as blocks. Stable
  // reference across renders so the block memos below don't invalidate.
  const manualActivities = useMemo(
    () => activities.filter((a) => a.source === "manual" && a.ends_at),
    [activities],
  );

  // Every recorded activity (auto or manual) as a span in hours-from-midnight.
  // A "gap of no activity" is any time not covered by one of these.
  const occupiedRanges = useMemo(
    () =>
      activities
        .filter((a) => a.ends_at)
        .map((a) => ({
          start: Math.max(
            0,
            (new Date(a.starts_at).getTime() - selectedDate.getTime()) /
              3_600_000,
          ),
          end: Math.min(
            24,
            (new Date(a.ends_at!).getTime() - selectedDate.getTime()) /
              3_600_000,
          ),
        })),
    [activities, selectedDate],
  );

  // Hovering an empty gap on the timeline bar shows a ghost spanning the *exact*
  // gap between the surrounding recorded activities. For today, gaps stop at the
  // current time (the future isn't a fillable gap).
  function updateGhost(e: React.MouseEvent<HTMLDivElement>) {
    const rect = e.currentTarget.getBoundingClientRect();
    if (rect.width === 0) return;
    const cursorH = clamp((e.clientX - rect.left) / rect.width, 0, 1) * 24;

    const dayLimit = isToday
      ? clamp((now.getTime() - selectedDate.getTime()) / 3_600_000, 0, 24)
      : 24;

    // Over an existing activity, or in the not-yet-happened future → no ghost.
    if (
      cursorH >= dayLimit ||
      occupiedRanges.some((r) => cursorH >= r.start && cursorH < r.end)
    ) {
      setGhost(null);
      return;
    }

    const startH = Math.max(
      0,
      ...occupiedRanges.filter((r) => r.end <= cursorH).map((r) => r.end),
    );
    const endH = Math.min(
      dayLimit,
      ...occupiedRanges.filter((r) => r.start >= cursorH).map((r) => r.start),
    );
    if (endH <= startH) {
      setGhost(null);
      return;
    }

    const toDate = (h: number) =>
      new Date(selectedDate.getTime() + h * 3_600_000);
    setGhost((prev) =>
      prev &&
      prev.leftPct === (startH / 24) * 100 &&
      prev.widthPct === ((endH - startH) / 24) * 100
        ? prev
        : {
            leftPct: (startH / 24) * 100,
            widthPct: ((endH - startH) / 24) * 100,
            start: toDate(startH),
            end: toDate(endH),
          },
    );
  }

  function openAdd(span: { start: Date; end: Date }) {
    setAddSpan(span);
    setAddKey((k) => k + 1);
    setAddOpen(true);
    setGhost(null);
  }

  // Memoized rendered block arrays. Crucially these do NOT depend on `zoom` —
  // each block is positioned by `left:%` / `width:%` within the inner div, so
  // when zoom changes the inner div widens and percentages scale naturally.
  // React reuses these JSX nodes across zoom-triggered renders, skipping the
  // .map(), positionInDay() calls, and Tooltip reconciliation.
  const activityBlocks = useMemo(
    () =>
      manualActivities.map((a) => {
        const { leftPct, widthPct } = positionInDay(
          a.starts_at,
          a.ends_at!,
          selectedDate,
        );
        if (widthPct === 0) return null;
        return (
          <Tooltip key={a.id}>
            <TooltipTrigger asChild>
              <div
                role="button"
                onClick={() => setSelected(a)}
                className="bg-primary/90 text-primary-foreground hover:bg-primary absolute top-0 z-20 flex h-full cursor-pointer items-center overflow-hidden text-[10px] transition-colors"
                style={{ left: `${leftPct}%`, width: `${widthPct}%` }}
              >
                {/* px on the span (clipped on tiny blocks) so the block width
                    never floors at the padding — it tracks the slot exactly. */}
                <span className="truncate px-1.5">{a.title}</span>
              </div>
            </TooltipTrigger>
            <TooltipContent>
              <div className="font-medium">{a.title}</div>
              <div className="text-muted-foreground">
                {formatTime(a.starts_at)} – {formatTime(a.ends_at!)}
              </div>
            </TooltipContent>
          </Tooltip>
        );
      }),
    [manualActivities, selectedDate],
  );

  const segmentBlocks = useMemo(
    () =>
      autoActivities.map((a) => {
        const { leftPct, widthPct } = positionInDay(
          a.starts_at,
          a.ends_at!,
          selectedDate,
        );
        if (widthPct === 0) return null;
        const durSeconds =
          (new Date(a.ends_at!).getTime() - new Date(a.starts_at).getTime()) /
          1000;
        return (
          <Tooltip key={a.id}>
            <TooltipTrigger asChild>
              <div
                role="button"
                onClick={() => setSelected(a)}
                className="border-background/30 absolute top-0 h-full cursor-pointer border-r last:border-r-0"
                style={{
                  left: `${leftPct}%`,
                  width: `${widthPct}%`,
                  backgroundColor: appColor(a.app_name!, a.is_active!),
                }}
              />
            </TooltipTrigger>
            <TooltipContent>
              <div className="font-medium">{a.app_name}</div>
              <div className="text-muted-foreground">
                {formatTime(a.starts_at)} – {formatTime(a.ends_at!)} ·{" "}
                {formatDuration(durSeconds)}
                {!a.is_active && " · idle"}
              </div>
            </TooltipContent>
          </Tooltip>
        );
      }),
    [autoActivities, selectedDate],
  );

  return (
    <div className="space-y-6">
      <DayNavHeader selectedDate={selectedDate} onSelectDate={selectDay} />

      <div className="flex flex-wrap items-center justify-between gap-3">
        <p className="text-muted-foreground text-sm">
          {formatDuration(totalActive)} active across {autoActivities.length}{" "}
          segment{autoActivities.length === 1 ? "" : "s"}
        </p>
        <DeviceScopeToggle value={scope} onChange={setScope} />
      </div>

      {error && (
        <Alert variant="destructive">
          <AlertDescription>{error}</AlertDescription>
        </Alert>
      )}

      <Card ref={cardRef}>
        <CardHeader className="flex flex-row items-center justify-between space-y-0">
          <SectionLabel>Day at a glance</SectionLabel>
          <div className="flex items-center gap-2">
            {/* Pan: jump to the start / end of the (zoomed) day */}
            <div className="flex items-center gap-0.5">
              <Button
                variant="outline"
                size="icon-sm"
                onClick={jumpToStart}
                disabled={zoom <= MIN_ZOOM}
                aria-label="Jump to start"
                title="Jump to start (00:00)"
              >
                <ChevronFirst />
              </Button>
              <Button
                variant="outline"
                size="icon-sm"
                onClick={jumpToEnd}
                disabled={zoom <= MIN_ZOOM}
                aria-label="Jump to end"
                title="Jump to end"
              >
                <ChevronLast />
              </Button>
            </div>

            {/* Quick-zoom presets; the one matching the current window fills in */}
            <div className="flex items-center gap-0.5">
              {QUICK_WINDOW_HOURS.map((h) => {
                const active = Math.abs(windowHours(zoom) - h) < 0.01;
                return (
                  <Button
                    key={h}
                    variant={active ? "default" : "outline"}
                    size="sm"
                    className="tabular-nums"
                    onClick={() => zoomToWindow(h)}
                    aria-label={`Zoom to ${h}-hour window`}
                  >
                    {h}h
                  </Button>
                );
              })}
            </div>

            {/* Main zoom control: a segmented −/window/+ module, bordered as one
                unit so it reads as the primary zoom representation. The center
                shows the live window and resets to the full day on click. */}
            <div className="divide-border flex items-center divide-x overflow-hidden rounded-lg border">
              <Button
                variant="ghost"
                size="icon-sm"
                className="rounded-none border-0"
                onClick={() => zoomFromButton(1 / BUTTON_ZOOM_FACTOR)}
                disabled={zoom <= MIN_ZOOM}
                aria-label="Zoom out"
              >
                <Minus />
              </Button>
              <Button
                variant="ghost"
                size="sm"
                className="min-w-14 rounded-none border-0 tabular-nums"
                onClick={resetZoom}
                aria-label="Reset zoom"
                title="Reset zoom"
              >
                {formatWindow(zoom)}
              </Button>
              <Button
                variant="ghost"
                size="icon-sm"
                className="rounded-none border-0"
                onClick={() => zoomFromButton(BUTTON_ZOOM_FACTOR)}
                disabled={zoom >= MAX_ZOOM}
                aria-label="Zoom in"
              >
                <Plus />
              </Button>
            </div>
          </div>
        </CardHeader>
        <CardContent>
          <div
            ref={scrollRef}
            className="overflow-x-auto overflow-y-hidden"
            style={{ overscrollBehaviorX: "contain" }}
          >
            <div
              ref={innerRef}
              className="space-y-1"
              style={{ minWidth: "100%" }}
            >
              {/* Single timeline bar: auto-tracked usage as the background
                  texture, manual activities as labeled foreground blocks (z-20),
                  and a ghost spanning any true empty gap (click-to-add). */}
              <div
                className="bg-muted/30 relative h-12 rounded"
                onMouseMove={updateGhost}
                onMouseLeave={() => setGhost(null)}
              >
                {segmentBlocks}
                {activityBlocks}
                {ghost && (
                  <div
                    role="button"
                    aria-label="Add activity in this gap"
                    onClick={() =>
                      openAdd({ start: ghost.start, end: ghost.end })
                    }
                    className="border-primary/50 bg-primary/10 text-primary hover:bg-primary/20 absolute top-0 z-10 flex h-full cursor-pointer items-center justify-center overflow-hidden rounded border border-dashed transition-colors"
                    style={{
                      left: `${ghost.leftPct}%`,
                      width: `${ghost.widthPct}%`,
                    }}
                  >
                    <Plus className="size-3.5 shrink-0" />
                  </div>
                )}
                {nowPct !== null && (
                  <div
                    className="bg-foreground pointer-events-none absolute top-0 h-full w-px"
                    style={{ left: `${nowPct}%` }}
                  />
                )}
              </div>

              <div className="relative mt-1 h-4">
                {ticks.map((h) => {
                  const isEdge = h === 0 || h === 24;
                  return (
                    <span
                      key={h}
                      className={`text-muted-foreground absolute top-0 text-[10px] tabular-nums ${
                        isEdge ? "" : "-translate-x-1/2"
                      } ${h === 24 ? "right-0" : ""}`}
                      style={
                        h === 24 ? undefined : { left: `${(h / 24) * 100}%` }
                      }
                    >
                      {formatTick(h)}
                    </span>
                  );
                })}
              </div>
            </div>
          </div>

          {activities.length === 0 && !error && (
            <p className="text-muted-foreground mt-4 text-sm">
              {isToday
                ? "No tracked activity yet today."
                : "No tracked activity on this day."}
            </p>
          )}
        </CardContent>
      </Card>

      <ActivityDetail
        activity={selected}
        onOpenChange={(open) => !open && setSelected(null)}
        onChanged={() => setRefreshTick((n) => n + 1)}
      />

      {/* Remount per open so each ghost click re-seeds the pre-filled times. */}
      <LogActivity
        key={addKey}
        open={addOpen}
        onOpenChange={setAddOpen}
        defaultDate={selectedDate}
        defaultStart={addSpan?.start}
        defaultEnd={addSpan?.end}
        onCreated={() => {
          setAddOpen(false);
          setRefreshTick((n) => n + 1);
        }}
      />
    </div>
  );
}
