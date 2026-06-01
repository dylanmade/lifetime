import * as React from "react";

import { cn } from "@/lib/utils";

// Small uppercase "eyebrow" label that titles a card or section surface.
// Exists purely for visual consistency across otherwise-unrelated sections —
// see docs/design-system.md §5. Use anywhere the muted, tracked, uppercase
// section heading is wanted (inside a CardHeader, a collapsible trigger, etc.).
function SectionLabel({ className, ...props }: React.ComponentProps<"div">) {
  return (
    <div
      data-slot="section-label"
      className={cn(
        "text-muted-foreground text-xs font-medium tracking-wider uppercase",
        className,
      )}
      {...props}
    />
  );
}

export { SectionLabel };
