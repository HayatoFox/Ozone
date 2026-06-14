import * as RT from "@radix-ui/react-tooltip";
import type { ReactNode } from "react";
import { OVERLAY_ANIM } from "../../lib/anim";

export const TooltipProvider = RT.Provider;

// Tooltip sombre façon Discord. `label` = contenu ; enveloppe l'élément déclencheur.
export function Tip({
  label,
  side = "top",
  children,
}: {
  label: ReactNode;
  side?: "top" | "right" | "bottom" | "left";
  children: ReactNode;
}) {
  return (
    <RT.Root>
      <RT.Trigger asChild>{children}</RT.Trigger>
      <RT.Portal>
        <RT.Content
          side={side}
          sideOffset={8}
          className={`z-[60] rounded-lg bg-floating px-2.5 py-1.5 text-sm font-medium text-header shadow-pop ring-1 ring-line ${OVERLAY_ANIM}`}
        >
          {label}
          <RT.Arrow className="fill-floating" />
        </RT.Content>
      </RT.Portal>
    </RT.Root>
  );
}
