import * as Popover from "@radix-ui/react-popover";
import type { ReactNode } from "react";
import { Sticker as StickerIcon } from "lucide-react";
import type { Sticker } from "../../types";
import { mediaUrl } from "../../lib/instance";
import { OVERLAY_ANIM } from "../../lib/anim";

// Sélecteur d'autocollants de la guilde : un clic = envoi immédiat (comportement Discord).
export function StickerPicker({
  trigger,
  stickers,
  onPick,
}: {
  trigger: ReactNode;
  stickers: Sticker[];
  onPick: (sticker: Sticker) => void;
}) {
  return (
    <Popover.Root>
      <Popover.Trigger asChild>{trigger}</Popover.Trigger>
      <Popover.Portal>
        <Popover.Content
          side="top"
          align="end"
          sideOffset={6}
          className={`z-[60] max-h-[340px] w-[296px] overflow-y-auto rounded-xl bg-floating p-2 shadow-pop ring-1 ring-line scroll-thin ${OVERLAY_ANIM}`}
        >
          <div className="px-1 pb-1 text-xs font-semibold uppercase text-muted">Autocollants</div>
          {stickers.length === 0 ? (
            <div className="flex flex-col items-center gap-2 py-6 text-center text-sm text-muted">
              <StickerIcon size={22} />
              Ce serveur n'a pas encore d'autocollant.
            </div>
          ) : (
            <div className="grid grid-cols-3 gap-1.5">
              {stickers.map((st) => (
                <Popover.Close
                  key={st.id}
                  title={st.name}
                  onClick={() => onPick(st)}
                  className="pressable flex aspect-square items-center justify-center rounded-lg p-1 transition-transform hover:scale-105 hover:bg-hover"
                >
                  <img
                    src={mediaUrl(`/api/stickers/${st.id}`)}
                    alt={st.name}
                    className="h-full w-full object-contain"
                    draggable={false}
                  />
                </Popover.Close>
              ))}
            </div>
          )}
          <Popover.Arrow className="fill-floating" />
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
}
