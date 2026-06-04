import { useEffect, useMemo, useState } from "react";
import { Plus } from "lucide-react";
import { Card, CardContent, CardHeader } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { SectionLabel } from "@/components/section-label";
import { DayNavHeader } from "@/components/day-nav-header";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import {
  type AppDuration,
  type HourActivity,
  type ResolvedActivity,
  getActivitiesBetween,
  getAppTotals,
  getHourlyActivity,
} from "./api";
import { LogActivity } from "./LogActivity";
import { ActivityDetail } from "./ActivityDetail";
import {
  addDays,
  formatDuration,
  formatTimeRange,
  isSameDay,
  startOfDay,
} from "@/lib/datetime";

const POLL_INTERVAL_MS = 10_000;

function pad2(n: number): string {
  return String(n).padStart(2, "0");
}

export function Summary() {
  const [selectedDate, setSelectedDate] = useState(() =>
    startOfDay(new Date()),
  );
  const [totals, setTotals] = useState<AppDuration[]>([]);
  const [hourly, setHourly] = useState<HourActivity[]>([]);
  const [activities, setActivities] = useState<ResolvedActivity[]>([]);
  const [selected, setSelected] = useState<ResolvedActivity | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [showLog, setShowLog] = useState(false);
  const [refreshTick, setRefreshTick] = useState(0);

  const isToday = useMemo(
    () => isSameDay(selectedDate, new Date()),
    [selectedDate],
  );

  useEffect(() => {
    let cancelled = false;

    async function fetchAll() {
      try {
        const start = selectedDate;
        const end = isToday ? new Date() : addDays(selectedDate, 1);
        const dayEnd = addDays(selectedDate, 1);
        const [t, h, a] = await Promise.all([
          getAppTotals(start.toISOString(), end.toISOString()),
          getHourlyActivity(start.toISOString(), end.toISOString()),
          getActivitiesBetween(start.toISOString(), dayEnd.toISOString()),
        ]);
        if (!cancelled) {
          setTotals(t);
          setHourly(h);
          setActivities(a);
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
    const id = setInterval(fetchAll, POLL_INTERVAL_MS);
    return () => {
      cancelled = true;
      clearInterval(id);
    };
  }, [selectedDate, isToday, refreshTick]);

  const totalActive = totals.reduce((sum, t) => sum + t.active_seconds, 0);
  const maxApp = Math.max(1, ...totals.map((t) => t.active_seconds));
  const maxHour = Math.max(1, ...hourly.map((h) => h.active_seconds));
  const hasAnyHourly = hourly.some((h) => h.active_seconds > 0);
  // The Activities card lists user-logged (manual) activities; auto-tracked
  // usage lives in the breakdowns above and on the Timeline.
  const manualActivities = useMemo(
    () => activities.filter((a) => a.source === "manual"),
    [activities],
  );

  return (
    <div className="space-y-6">
      <DayNavHeader
        selectedDate={selectedDate}
        onSelectDate={(date) => setSelectedDate(startOfDay(date))}
      />

      <p className="text-muted-foreground text-sm">
        {formatDuration(totalActive)} active across {totals.length} app
        {totals.length === 1 ? "" : "s"}
      </p>

      {error && (
        <Alert variant="destructive">
          <AlertDescription>{error}</AlertDescription>
        </Alert>
      )}

      {totals.length === 0 && !error && (
        <p className="text-muted-foreground text-sm">
          {isToday
            ? "No tracked activity yet today."
            : "No tracked activity on this day."}
        </p>
      )}

      <div className="grid gap-6 lg:grid-cols-2">
        {hasAnyHourly && (
          <Card className="lg:col-span-2">
            <CardHeader>
              <SectionLabel>By hour</SectionLabel>
            </CardHeader>
            <CardContent>
              <div className="relative flex h-20 items-end gap-0.5 pb-5">
                {hourly.map((h) => {
                  const minutes = Math.round(h.active_seconds / 60);
                  const heightPct = (h.active_seconds / maxHour) * 100;
                  return (
                    <Tooltip key={h.hour}>
                      <TooltipTrigger asChild>
                        <div className="relative flex h-full flex-1 flex-col justify-end">
                          <div
                            className="bg-primary min-h-px rounded-t-sm transition-all"
                            style={{ height: `${heightPct}%` }}
                          />
                          {h.hour % 3 === 0 && (
                            <span className="text-muted-foreground absolute top-full left-1/2 mt-1 -translate-x-1/2 text-[10px] tabular-nums">
                              {pad2(h.hour)}
                            </span>
                          )}
                        </div>
                      </TooltipTrigger>
                      <TooltipContent>
                        {pad2(h.hour)}:00 · {minutes}m
                      </TooltipContent>
                    </Tooltip>
                  );
                })}
              </div>
            </CardContent>
          </Card>
        )}

        {totals.length > 0 && (
          <Card className="lg:col-span-2">
            <CardHeader>
              <SectionLabel>By app</SectionLabel>
            </CardHeader>
            <CardContent>
              <ul className="space-y-3">
                {totals.map((t) => (
                  <li key={t.app_name}>
                    <div className="mb-1.5 flex items-baseline justify-between">
                      <span className="text-sm font-medium">{t.app_name}</span>
                      <span className="text-muted-foreground text-xs tabular-nums">
                        {formatDuration(t.active_seconds)}
                      </span>
                    </div>
                    <div className="bg-muted h-1 overflow-hidden rounded-full">
                      <div
                        className="bg-primary h-full transition-all"
                        style={{
                          width: `${(t.active_seconds / maxApp) * 100}%`,
                        }}
                      />
                    </div>
                  </li>
                ))}
              </ul>
            </CardContent>
          </Card>
        )}

        <Card className="lg:col-span-2">
          <CardHeader>
            <SectionLabel>Activities</SectionLabel>
          </CardHeader>
          <CardContent className="space-y-3">
            {manualActivities.length === 0 ? (
              <p className="text-muted-foreground text-sm">
                No activities logged for this day.
              </p>
            ) : (
              <ul>
                {manualActivities.map((a) => (
                  <li key={a.id}>
                    <button
                      type="button"
                      onClick={() => setSelected(a)}
                      className="hover:bg-muted/40 grid w-full grid-cols-[140px_1fr] gap-4 border-b py-2 text-left transition-colors last:border-b-0"
                    >
                      <span className="text-muted-foreground font-mono text-xs tabular-nums">
                        {formatTimeRange(a.starts_at, a.ends_at)}
                      </span>
                      <div className="min-w-0">
                        <div className="flex items-center gap-2">
                          <p className="truncate text-sm font-medium">
                            {a.title}
                          </p>
                          {a.category && (
                            <Badge variant="secondary" className="shrink-0">
                              {a.category}
                            </Badge>
                          )}
                        </div>
                        {a.description && (
                          <p className="text-muted-foreground mt-1 truncate text-sm">
                            {a.description}
                          </p>
                        )}
                      </div>
                    </button>
                  </li>
                ))}
              </ul>
            )}
            <Button
              variant="outline"
              size="sm"
              onClick={() => setShowLog(true)}
            >
              <Plus />
              Add manual activity
            </Button>
            <p className="text-muted-foreground text-xs">
              Auto-tracked app usage also appears as activities on the Timeline.
            </p>
          </CardContent>
        </Card>
      </div>

      <LogActivity
        open={showLog}
        onOpenChange={setShowLog}
        defaultDate={selectedDate}
        onCreated={() => {
          setShowLog(false);
          setRefreshTick((n) => n + 1);
        }}
      />

      <ActivityDetail
        activity={selected}
        onOpenChange={(open) => !open && setSelected(null)}
        onChanged={() => setRefreshTick((n) => n + 1)}
      />
    </div>
  );
}
