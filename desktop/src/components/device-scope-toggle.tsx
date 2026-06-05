import { Button } from "@/components/ui/button";

export type DeviceScope = "local" | "all";

// Choose whether a data view shows only this device's timeline (default) or all
// synced devices amalgamated. See docs / sync-architecture: synced data is never
// force-merged — amalgamation is an explicit, opt-in choice here.
export function DeviceScopeToggle({
  value,
  onChange,
}: {
  value: DeviceScope;
  onChange: (value: DeviceScope) => void;
}) {
  return (
    <div className="flex items-center gap-0.5">
      <Button
        variant={value === "local" ? "default" : "outline"}
        size="sm"
        onClick={() => onChange("local")}
      >
        This device
      </Button>
      <Button
        variant={value === "all" ? "default" : "outline"}
        size="sm"
        onClick={() => onChange("all")}
      >
        All devices
      </Button>
    </div>
  );
}

/// The `deviceId` argument to pass to the read commands for a given scope.
export function scopeArg(scope: DeviceScope): string | undefined {
  return scope === "all" ? "all" : undefined;
}
