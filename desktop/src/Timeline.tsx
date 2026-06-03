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
import {
  type Activity,
  type AppSegment,
  getActivitiesBetween,
  getTimelineSegments,
} from "./api";
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
  const [segments, setSegments] = useState<AppSegment[]>([]);
  const [activities, setActivities] = useState<Activity[]>([]);
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
        const [segs, acts] = await Promise.all([
          getTimelineSegments(selectedDate.toISOString(), end.toISOString()),
          getActivitiesBetween(
            selectedDate.toISOString(),
            dayEnd.toISOString(),
          ),
        ]);
        if (!cancelled) {
          setSegments(segs);
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
  }, [selectedDate, isToday]);

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

  const totalActive = useMemo(
    () =>
      segments
        .filter((s) => s.is_active)
        .reduce(
          (sum, s) =>
            sum +
            (new Date(s.ends_at).getTime() - new Date(s.starts_at).getTime()) /
              1000,
          0,
        ),
    [segments],
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

  // Stable reference across renders so the block memos below don't invalidate
  // every render.
  const completedActivities = useMemo(
    () => activities.filter((a) => a.ends_at),
    [activities],
  );

  // Memoized rendered block arrays. Crucially these do NOT depend on `zoom` —
  // each block is positioned by `left:%` / `width:%` within the inner div, so
  // when zoom changes the inner div widens and percentages scale naturally.
  // React reuses these JSX nodes across zoom-triggered renders, skipping the
  // .map(), positionInDay() calls, and Tooltip reconciliation.
  const activityBlocks = useMemo(
    () =>
      completedActivities.map((a) => {
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
                className="bg-primary/70 text-primary-foreground hover:bg-primary absolute top-0 flex h-full cursor-default items-center overflow-hidden rounded px-1.5 text-[10px] transition-colors"
                style={{ left: `${leftPct}%`, width: `${widthPct}%` }}
              >
                <span className="truncate">{a.title ?? "(untitled)"}</span>
              </div>
            </TooltipTrigger>
            <TooltipContent>
              <div className="font-medium">{a.title ?? "(untitled)"}</div>
              <div className="text-muted-foreground">
                {formatTime(a.starts_at)} – {formatTime(a.ends_at!)}
              </div>
            </TooltipContent>
          </Tooltip>
        );
      }),
    [completedActivities, selectedDate],
  );

  const segmentBlocks = useMemo(
    () =>
      segments.map((s, i) => {
        const { leftPct, widthPct } = positionInDay(
          s.starts_at,
          s.ends_at,
          selectedDate,
        );
        if (widthPct === 0) return null;
        const durSeconds =
          (new Date(s.ends_at).getTime() - new Date(s.starts_at).getTime()) /
          1000;
        return (
          <Tooltip key={i}>
            <TooltipTrigger asChild>
              <div
                className="border-background/30 absolute top-0 h-full cursor-default border-r last:border-r-0"
                style={{
                  left: `${leftPct}%`,
                  width: `${widthPct}%`,
                  backgroundColor: appColor(s.app_name, s.is_active),
                }}
              />
            </TooltipTrigger>
            <TooltipContent>
              <div className="font-medium">{s.app_name}</div>
              <div className="text-muted-foreground">
                {formatTime(s.starts_at)} – {formatTime(s.ends_at)} ·{" "}
                {formatDuration(durSeconds)}
                {!s.is_active && " · idle"}
              </div>
            </TooltipContent>
          </Tooltip>
        );
      }),
    [segments, selectedDate],
  );

  return (
    <div className="space-y-6">
      <DayNavHeader selectedDate={selectedDate} onSelectDate={selectDay} />

      <p className="text-muted-foreground text-sm">
        {formatDuration(totalActive)} active across {segments.length} segment
        {segments.length === 1 ? "" : "s"}
      </p>

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
              {completedActivities.length > 0 && (
                <div className="bg-muted/30 relative h-6 rounded">
                  {activityBlocks}
                  {nowPct !== null && (
                    <div
                      className="bg-foreground pointer-events-none absolute top-0 h-full w-px"
                      style={{ left: `${nowPct}%` }}
                    />
                  )}
                </div>
              )}

              <div className="bg-muted/30 relative h-12 rounded">
                {segmentBlocks}
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

          {segments.length === 0 && !error && (
            <p className="text-muted-foreground mt-4 text-sm">
              {isToday
                ? "No tracked activity yet today."
                : "No tracked activity on this day."}
            </p>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
