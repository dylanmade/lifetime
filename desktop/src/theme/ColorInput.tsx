import { useEffect, useState } from "react";
import { HexColorPicker } from "react-colorful";
import { RotateCcw } from "lucide-react";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import { Button } from "@/components/ui/button";
import { readTokenHex } from "./colors";
import { useTheme } from "./ThemeProvider";
import type { ColorToken } from "./tokens";

type Props = {
  token: ColorToken;
  label: string;
};

export function ColorInput({ token, label }: Props) {
  const { activeMode, overrides, setColor, clearColor } = useTheme();
  const isOverridden = overrides.colors[activeMode][token] !== undefined;

  // The "displayed" color: either the user's override or the resolved
  // computed value of the underlying CSS variable.
  const [display, setDisplay] = useState<string>(() => readTokenHex(token));

  // Sync the displayed swatch with whatever the DOM is currently showing.
  // Re-read on mode change, on override change, and on initial mount.
  useEffect(() => {
    // requestAnimationFrame so we read AFTER the provider's DOM write.
    const id = requestAnimationFrame(() => setDisplay(readTokenHex(token)));
    return () => cancelAnimationFrame(id);
  }, [activeMode, isOverridden, overrides.colors, token]);

  return (
    <div className="flex items-center justify-between gap-3 py-1">
      <span className="text-sm">{label}</span>
      <div className="flex items-center gap-1">
        {isOverridden && (
          <Button
            variant="ghost"
            size="icon"
            className="h-7 w-7"
            onClick={() => clearColor(token)}
            aria-label={`Reset ${label}`}
            title="Reset to default"
          >
            <RotateCcw className="size-3.5" />
          </Button>
        )}
        <Popover>
          <PopoverTrigger asChild>
            <button
              type="button"
              className="border-border ring-offset-background focus-visible:ring-ring h-8 w-14 rounded-md border focus-visible:ring-2 focus-visible:ring-offset-2 focus-visible:outline-none"
              style={{ backgroundColor: display }}
              aria-label={`Edit ${label}`}
            />
          </PopoverTrigger>
          <PopoverContent className="w-auto p-3" align="end">
            <HexColorPicker
              color={display}
              onChange={(next) => setColor(token, next)}
            />
            <div className="mt-2 flex items-center justify-between gap-2">
              <span className="text-muted-foreground font-mono text-xs uppercase">
                {display}
              </span>
              {isOverridden && (
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => clearColor(token)}
                >
                  Reset
                </Button>
              )}
            </div>
          </PopoverContent>
        </Popover>
      </div>
    </div>
  );
}
