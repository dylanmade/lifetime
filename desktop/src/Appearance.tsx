import { useState } from "react";
import { ChevronDown, Monitor, Moon, RotateCcw, Sun } from "lucide-react";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Slider } from "@/components/ui/slider";
import { ColorInput } from "@/theme/ColorInput";
import { FontPicker } from "@/theme/FontPicker";
import { ProfileSection } from "@/theme/ProfileSection";
import { useTheme, type ThemeMode } from "@/theme/ThemeProvider";
import {
  COLOR_GROUPS,
  DEFAULT_RADIUS_REM,
  MAX_RADIUS_REM,
  MIN_RADIUS_REM,
  MONO_FONTS,
  SANS_FONTS,
} from "@/theme/tokens";

const MODE_OPTIONS: { value: ThemeMode; label: string; icon: typeof Sun }[] = [
  { value: "light", label: "Light", icon: Sun },
  { value: "dark", label: "Dark", icon: Moon },
  { value: "system", label: "System", icon: Monitor },
];

export function Appearance() {
  const {
    mode,
    activeMode,
    overrides,
    setMode,
    setRadius,
    setFontSans,
    setFontHeading,
    setFontMono,
    reset,
  } = useTheme();

  const radius = overrides.radiusRem ?? DEFAULT_RADIUS_REM;

  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-center justify-between gap-4">
        <div>
          <h1 className="text-2xl font-semibold tracking-tight">Appearance</h1>
          <p className="text-muted-foreground text-sm">
            Editing{" "}
            <span className="text-foreground font-medium">{activeMode}</span>{" "}
            mode. Changes apply live and persist across sessions.
          </p>
        </div>
        <Button variant="outline" onClick={reset}>
          <RotateCcw className="mr-2 h-4 w-4" />
          Reset all
        </Button>
      </div>

      <ProfileSection />

      <Card>
        <CardHeader>
          <CardTitle className="text-muted-foreground text-xs font-medium tracking-wider uppercase">
            Mode
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="flex flex-wrap gap-2">
            {MODE_OPTIONS.map(({ value, label, icon: Icon }) => (
              <Button
                key={value}
                variant={mode === value ? "default" : "outline"}
                onClick={() => setMode(value)}
              >
                <Icon className="mr-2 h-4 w-4" />
                {label}
              </Button>
            ))}
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="text-muted-foreground text-xs font-medium tracking-wider uppercase">
            Typography
          </CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          <FontRow label="Sans (body)">
            <FontPicker
              value={overrides.fontSans ?? SANS_FONTS[0].id}
              bundled={SANS_FONTS}
              onChange={setFontSans}
            />
          </FontRow>
          <FontRow label="Heading">
            <FontPicker
              value={overrides.fontHeading ?? SANS_FONTS[0].id}
              bundled={SANS_FONTS}
              onChange={setFontHeading}
            />
          </FontRow>
          <FontRow label="Mono">
            <FontPicker
              value={overrides.fontMono ?? MONO_FONTS[0].id}
              bundled={MONO_FONTS}
              monoOnly
              onChange={setFontMono}
            />
          </FontRow>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="text-muted-foreground text-xs font-medium tracking-wider uppercase">
            Shape
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="flex items-center justify-between gap-4 py-1">
            <span className="text-sm">Radius</span>
            <span className="text-muted-foreground font-mono text-xs tabular-nums">
              {radius.toFixed(3)}rem
            </span>
          </div>
          <Slider
            value={[radius]}
            min={MIN_RADIUS_REM}
            max={MAX_RADIUS_REM}
            step={0.025}
            onValueChange={(v) => setRadius(v[0])}
            className="mt-2"
          />
        </CardContent>
      </Card>

      {COLOR_GROUPS.map((group) => (
        <ColorSection key={group.name} group={group} />
      ))}
    </div>
  );
}

function ColorSection({ group }: { group: (typeof COLOR_GROUPS)[number] }) {
  const [open, setOpen] = useState(group.defaultOpen);
  return (
    <Card className="gap-0 overflow-hidden py-0">
      <button
        type="button"
        onClick={() => setOpen(!open)}
        className="hover:bg-muted/40 flex w-full items-center justify-between px-5 py-4 text-left transition-colors"
        aria-expanded={open}
      >
        <span className="text-muted-foreground text-xs font-medium tracking-wider uppercase">
          {group.name}
        </span>
        <ChevronDown
          className={`text-muted-foreground h-4 w-4 transition-transform ${open ? "rotate-180" : ""}`}
        />
      </button>
      {open && (
        <div className="divide-border border-border divide-y border-t px-5 py-2">
          {group.tokens.map((t) => (
            <ColorInput key={t.id} token={t.id} label={t.label} />
          ))}
        </div>
      )}
    </Card>
  );
}

function FontRow({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <div className="flex items-center justify-between gap-3">
      <span className="text-sm">{label}</span>
      {children}
    </div>
  );
}
