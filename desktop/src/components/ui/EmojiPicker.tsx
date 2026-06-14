import * as Popover from "@radix-ui/react-popover";
import { useMemo, useRef, useState, type ReactNode } from "react";
import { Clock, Search } from "lucide-react";
import type { Emoji } from "../../types";
import { mediaUrl } from "../../lib/instance";
import { OVERLAY_ANIM } from "../../lib/anim";
import { ALL_EMOJIS, EMOJI_CATEGORIES } from "../../lib/emojiData";

// Émojis récemment utilisés (persistés, max 24).
const FREQ_KEY = "ozone.emojiFrequents";
function loadFrequents(): string[] {
  if (typeof localStorage === "undefined") return [];
  try {
    const v = JSON.parse(localStorage.getItem(FREQ_KEY) || "[]");
    return Array.isArray(v) ? v.slice(0, 24) : [];
  } catch {
    return [];
  }
}
function pushFrequent(e: string): void {
  if (typeof localStorage === "undefined") return;
  const cur = loadFrequents().filter((x) => x !== e);
  localStorage.setItem(FREQ_KEY, JSON.stringify([e, ...cur].slice(0, 24)));
}

export function EmojiPicker({
  trigger,
  onPick,
  custom,
}: {
  trigger: ReactNode;
  onPick: (emoji: string) => void;
  custom?: Emoji[];
}) {
  const [query, setQuery] = useState("");
  const frequents = useRef(loadFrequents());

  const q = query.trim().toLowerCase();
  const filtered = useMemo(
    () =>
      q ? ALL_EMOJIS.filter((x) => x.e === q || x.kw.includes(q)).map((x) => x.e) : null,
    [q],
  );
  const customFiltered = useMemo(
    () => (q && custom ? custom.filter((c) => c.name.toLowerCase().includes(q)) : custom),
    [q, custom],
  );

  function pick(e: string) {
    pushFrequent(e);
    onPick(e);
  }

  return (
    <Popover.Root>
      <Popover.Trigger asChild>{trigger}</Popover.Trigger>
      <Popover.Portal>
        <Popover.Content
          side="top"
          align="end"
          sideOffset={6}
          className={`z-[60] flex h-[380px] w-[320px] flex-col overflow-hidden rounded-xl bg-floating shadow-pop ring-1 ring-line ${OVERLAY_ANIM}`}
        >
          {/* Recherche */}
          <div className="flex shrink-0 items-center gap-2 border-b border-line px-3 py-2">
            <Search size={15} className="text-muted" />
            <input
              autoFocus
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Rechercher un émoji…"
              className="flex-1 bg-transparent text-sm text-normal outline-none placeholder:text-muted"
            />
          </div>

          <div className="flex-1 overflow-y-auto p-2 scroll-thin">
            {/* Résultats de recherche (à plat) */}
            {filtered ? (
              <>
                {customFiltered && customFiltered.length > 0 && (
                  <Section title="Personnalisés">
                    {customFiltered.map((e) => (
                      <Popover.Close
                        key={e.id}
                        title={`:${e.name}:`}
                        onClick={() => onPick(`<${e.animated ? "a" : ""}:${e.name}:${e.id}>`)}
                        className="pressable flex items-center justify-center rounded p-1 transition-transform hover:scale-[1.18] hover:bg-hover"
                      >
                        <img src={mediaUrl(`/api/emojis/${e.id}`)} alt={e.name} className="h-6 w-6 object-contain" />
                      </Popover.Close>
                    ))}
                  </Section>
                )}
                {filtered.length === 0 && (!customFiltered || customFiltered.length === 0) ? (
                  <p className="px-1 py-4 text-center text-sm text-muted">Aucun émoji trouvé.</p>
                ) : (
                  <Section title="Résultats">
                    {filtered.map((e) => (
                      <Popover.Close
                        key={e}
                        onClick={() => pick(e)}
                        className="pressable rounded p-1 text-xl transition-transform hover:scale-[1.25] hover:bg-hover"
                      >
                        {e}
                      </Popover.Close>
                    ))}
                  </Section>
                )}
              </>
            ) : (
              <>
                {/* Fréquents */}
                {frequents.current.length > 0 && (
                  <Section title="Fréquemment utilisés" icon={<Clock size={12} />}>
                    {frequents.current.map((e) => (
                      <Popover.Close
                        key={e}
                        onClick={() => pick(e)}
                        className="pressable rounded p-1 text-xl transition-transform hover:scale-[1.25] hover:bg-hover"
                      >
                        {e}
                      </Popover.Close>
                    ))}
                  </Section>
                )}
                {/* Personnalisés de la guilde */}
                {custom && custom.length > 0 && (
                  <Section title="Personnalisés">
                    {custom.map((e) => (
                      <Popover.Close
                        key={e.id}
                        title={`:${e.name}:`}
                        onClick={() => onPick(`<${e.animated ? "a" : ""}:${e.name}:${e.id}>`)}
                        className="pressable flex items-center justify-center rounded p-1 transition-transform hover:scale-[1.18] hover:bg-hover"
                      >
                        <img src={mediaUrl(`/api/emojis/${e.id}`)} alt={e.name} className="h-6 w-6 object-contain" />
                      </Popover.Close>
                    ))}
                  </Section>
                )}
                {/* Catégories unicode */}
                {EMOJI_CATEGORIES.map((cat) => (
                  <Section key={cat.id} title={cat.label}>
                    {cat.emojis.map((x) => (
                      <Popover.Close
                        key={x.e}
                        title={x.kw.split(" ")[0]}
                        onClick={() => pick(x.e)}
                        className="pressable rounded p-1 text-xl transition-transform hover:scale-[1.25] hover:bg-hover"
                      >
                        {x.e}
                      </Popover.Close>
                    ))}
                  </Section>
                ))}
              </>
            )}
          </div>
          <Popover.Arrow className="fill-floating" />
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
}

function Section({
  title,
  icon,
  children,
}: {
  title: string;
  icon?: ReactNode;
  children: ReactNode;
}) {
  return (
    <div className="mb-2">
      <div className="flex items-center gap-1 px-1 pb-1 text-xs font-semibold uppercase text-muted">
        {icon}
        {title}
      </div>
      <div className="grid grid-cols-8 gap-0.5">{children}</div>
    </div>
  );
}
