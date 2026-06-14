import { useState } from "react";
import * as Popover from "@radix-ui/react-popover";
import { AtSign, Pin, Search } from "lucide-react";
import { api } from "../api";
import { useStore } from "../store";
import type { Message, Snowflake } from "../types";
import { displayName, formatDayTime } from "../lib/format";
import { OVERLAY_ANIM } from "../lib/anim";
import { ListSkeleton } from "./ui/Skeleton";

// Résolution salon → nom (guilde « #salon » ou MP) pour situer chaque mention.
// Lecture PONCTUELLE du store (getState) : pas d'abonnement — un sélecteur qui renverrait une
// fonction créerait un snapshot toujours « neuf » et une boucle de re-rendu infinie.
function resolveChannelName(cid: Snowflake): string {
  const s = useStore.getState();
  for (const list of Object.values(s.channelsByGuild)) {
    const c = list.find((x) => x.id === cid);
    if (c) return `#${c.name}`;
  }
  const dm = s.dms.find((d) => d.id === cid);
  if (dm) return dm.name || "Message privé";
  return "";
}

// Boîte de réception : messages récents qui mentionnent l'utilisateur (tous salons confondus).
export function InboxButton() {
  const [items, setItems] = useState<Message[] | null>(null);

  async function load(open: boolean) {
    if (open) {
      setItems(null);
      try {
        setItems(await api.mentionsInbox(30));
      } catch {
        setItems([]);
      }
    }
  }

  return (
    <Popover.Root onOpenChange={(o) => void load(o)}>
      <Popover.Trigger
        title="Boîte de réception (mentions récentes)"
        className="pressable rounded p-1.5 text-interactive-normal outline-none transition-colors hover:bg-hover hover:text-interactive-hover"
      >
        <AtSign size={20} />
      </Popover.Trigger>
      <Popover.Portal>
        <Popover.Content
          side="bottom"
          align="end"
          sideOffset={8}
          className={`z-[60] max-h-[420px] w-[420px] overflow-y-auto rounded-xl bg-floating shadow-pop ring-1 ring-cardline scroll-thin ${OVERLAY_ANIM}`}
        >
          <div className="border-b border-line px-4 py-3 font-semibold text-header">
            Mentions récentes
          </div>
          {items === null ? (
            <ListSkeleton rows={3} />
          ) : items.length === 0 ? (
            <p className="p-4 text-sm text-muted">Personne ne t'a mentionné récemment.</p>
          ) : (
            <div className="p-2">
              {items.map((m) => (
                <div key={m.id} className="rounded-lg p-2 hover:bg-hover">
                  <div className="flex items-baseline gap-2">
                    <span className="text-sm font-medium text-header">{displayName(m.author)}</span>
                    {resolveChannelName(m.channel_id) && (
                      <span className="truncate text-xs text-link">{resolveChannelName(m.channel_id)}</span>
                    )}
                    <span className="ml-auto shrink-0 text-xs text-muted">{formatDayTime(m.created_at)}</span>
                  </div>
                  <p className="line-clamp-3 whitespace-pre-wrap break-words text-sm text-normal">
                    {m.content || "(pièce jointe)"}
                  </p>
                </div>
              ))}
            </div>
          )}
          <Popover.Arrow className="fill-floating" />
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
}

// Liste des messages épinglés du salon.
export function PinsButton({ channelId }: { channelId: string }) {
  const [pins, setPins] = useState<Message[] | null>(null);

  async function load(open: boolean) {
    if (open) {
      setPins(null);
      try {
        setPins(await api.listPins(channelId));
      } catch {
        setPins([]);
      }
    }
  }

  return (
    <Popover.Root onOpenChange={(o) => void load(o)}>
      <Popover.Trigger
        title="Messages épinglés"
        className="pressable rounded p-1.5 text-interactive-normal outline-none transition-colors hover:bg-hover hover:text-interactive-hover"
      >
        <Pin size={20} />
      </Popover.Trigger>
      <Popover.Portal>
        <Popover.Content
          side="bottom"
          align="end"
          sideOffset={8}
          className={`z-[60] max-h-[420px] w-[420px] overflow-y-auto rounded-xl bg-floating shadow-pop ring-1 ring-cardline scroll-thin ${OVERLAY_ANIM}`}
        >
          <div className="border-b border-line px-4 py-3 font-semibold text-header">
            Messages épinglés
          </div>
          {pins === null ? (
            <ListSkeleton rows={3} />
          ) : pins.length === 0 ? (
            <p className="p-4 text-sm text-muted">Aucun message épinglé.</p>
          ) : (
            <div className="p-2">
              {pins.map((m) => (
                <ResultRow key={m.id} message={m} />
              ))}
            </div>
          )}
          <Popover.Arrow className="fill-floating" />
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
}

// Recherche de messages dans le salon.
export function SearchButton({ channelId }: { channelId: string }) {
  const [q, setQ] = useState("");
  const [results, setResults] = useState<Message[] | null>(null);
  const [busy, setBusy] = useState(false);

  async function run() {
    const query = q.trim();
    if (!query) return;
    setBusy(true);
    try {
      const res = await api.searchChannel(channelId, query);
      setResults(res.messages);
    } catch {
      setResults([]);
    } finally {
      setBusy(false);
    }
  }

  return (
    <Popover.Root>
      <Popover.Trigger
        title="Rechercher"
        className="pressable rounded p-1.5 text-interactive-normal outline-none transition-colors hover:bg-hover hover:text-interactive-hover"
      >
        <Search size={20} />
      </Popover.Trigger>
      <Popover.Portal>
        <Popover.Content
          side="bottom"
          align="end"
          sideOffset={8}
          className={`z-[60] max-h-[460px] w-[440px] overflow-hidden rounded-xl bg-floating shadow-pop ring-1 ring-cardline ${OVERLAY_ANIM}`}
        >
          <div className="p-3">
            <input
              autoFocus
              value={q}
              onChange={(e) => setQ(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") void run();
              }}
              placeholder="Rechercher dans ce salon…"
              className="w-full rounded-lg bg-deepest px-3 py-2 text-normal outline-none ring-1 ring-transparent focus:ring-accent"
            />
          </div>
          <div className="max-h-[380px] overflow-y-auto px-2 pb-2 scroll-thin">
            {busy ? (
              <p className="p-3 text-sm text-muted">Recherche…</p>
            ) : results === null ? (
              <p className="p-3 text-sm text-muted">Tape une requête puis Entrée.</p>
            ) : results.length === 0 ? (
              <p className="p-3 text-sm text-muted">Aucun résultat.</p>
            ) : (
              results.map((m) => <ResultRow key={m.id} message={m} />)
            )}
          </div>
          <Popover.Arrow className="fill-floating" />
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
}

function ResultRow({ message }: { message: Message }) {
  return (
    <div className="rounded-lg p-2 hover:bg-hover">
      <div className="flex items-baseline gap-2">
        <span className="text-sm font-medium text-header">{displayName(message.author)}</span>
        <span className="text-xs text-muted">{formatDayTime(message.created_at)}</span>
      </div>
      <p className="line-clamp-3 whitespace-pre-wrap break-words text-sm text-normal">
        {message.content || "(pièce jointe)"}
      </p>
    </div>
  );
}
