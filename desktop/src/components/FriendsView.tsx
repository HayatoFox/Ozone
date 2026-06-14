import { useState } from "react";
import { Check, MessageCircle, Users, X } from "lucide-react";
import { api, ApiError } from "../api";
import { useStore } from "../store";
import { Avatar } from "./Avatar";
import { displayName } from "../lib/format";
import type { Relationship, RelationshipType } from "../types";

type Tab = "all" | "online" | "pending" | "blocked" | "add";

const ONLINE = new Set(["online", "idle", "dnd"]);

export function FriendsView() {
  const [tab, setTab] = useState<Tab>("all");
  const relationships = useStore((s) => s.relationships);
  const presences = useStore((s) => s.presences);

  const friends = relationships.filter((r) => r.type === "friend");
  const pending = relationships.filter((r) => r.type === "incoming" || r.type === "outgoing");
  const blocked = relationships.filter((r) => r.type === "blocked");

  let list: Relationship[] = friends;
  if (tab === "online") list = friends.filter((r) => ONLINE.has(presences[r.user.id] ?? "offline"));
  else if (tab === "pending") list = pending;
  else if (tab === "blocked") list = blocked;

  return (
    <div className="flex flex-1 flex-col bg-chat">
      <header className="flex h-12 shrink-0 items-center gap-4 border-b border-line px-4 shadow-sm">
        <div className="flex items-center gap-2 font-semibold text-header">
          <Users size={20} className="text-muted" />
          Amis
        </div>
        <span className="h-5 w-px bg-white/10" />
        <TabBtn active={tab === "online"} onClick={() => setTab("online")}>En ligne</TabBtn>
        <TabBtn active={tab === "all"} onClick={() => setTab("all")}>Tous</TabBtn>
        <TabBtn active={tab === "pending"} onClick={() => setTab("pending")}>
          En attente {pending.length > 0 && <Badge n={pending.length} />}
        </TabBtn>
        <TabBtn active={tab === "blocked"} onClick={() => setTab("blocked")}>Bloqués</TabBtn>
        <button
          onClick={() => setTab("add")}
          className={`rounded px-2 py-1 text-sm font-medium ${
            tab === "add" ? "bg-online/20 text-online" : "bg-online text-white hover:bg-online/90"
          }`}
        >
          Ajouter un ami
        </button>
      </header>

      {tab === "add" ? (
        <AddFriend />
      ) : (
        <div className="flex-1 overflow-y-auto px-6 py-4 scroll-thin">
          {list.length === 0 ? (
            <div className="mt-16 flex flex-col items-center gap-3 text-center">
              <div className="flex h-16 w-16 items-center justify-center rounded-2xl bg-hover ring-1 ring-line surface-card">
                <Users size={30} style={{ color: "var(--aurora-a)" }} />
              </div>
              <p className="text-sm text-muted">Personne ici pour l'instant.</p>
            </div>
          ) : (
            list.map((r) => <FriendRow key={r.id} rel={r} status={presences[r.user.id]} />)
          )}
        </div>
      )}
    </div>
  );
}

function FriendRow({ rel, status }: { rel: Relationship; status?: string }) {
  const removeRelationship = useStore((s) => s.removeRelationship);
  const acceptRelationship = useStore((s) => s.acceptRelationship);
  const openDM = useStore((s) => s.openDM);

  async function startDM() {
    const dm = await api.openDM({ recipients: [rel.user.id] });
    await useStore.getState().refreshDMs();
    await openDM(dm.id);
  }

  return (
    // Ligne arrondie au survol (les actions ne se révèlent qu'au survol, façon liste moderne).
    <div className="group -mx-2 flex items-center gap-3 rounded-xl px-2 py-2 transition-colors hover:bg-hover">
      <Avatar name={displayName(rel.user)} id={rel.user.id} size={36} status={status ?? "offline"} avatarId={rel.user.avatar_id} />
      <div className="min-w-0 flex-1">
        <div className="font-medium text-header">{displayName(rel.user)}</div>
        <div className="text-xs text-muted">{relLabel(rel.type, status)}</div>
      </div>
      <div className="flex gap-2 opacity-60 transition-opacity group-hover:opacity-100">
        {rel.type === "incoming" && (
          <button
            onClick={() => void acceptRelationship(rel.user.id)}
            className="rounded-full bg-active p-2 text-online ring-1 ring-line hover:bg-online hover:text-white"
            title="Accepter"
          >
            <Check size={18} />
          </button>
        )}
        {rel.type === "friend" && (
          <button
            onClick={() => void startDM()}
            className="rounded-full bg-active p-2 text-interactive-normal ring-1 ring-line hover:bg-selected hover:text-interactive-hover"
            title="Message"
          >
            <MessageCircle size={18} />
          </button>
        )}
        <button
          onClick={() => void removeRelationship(rel.user.id)}
          className="rounded-full bg-active p-2 text-muted ring-1 ring-line hover:bg-dnd/20 hover:text-dnd"
          title="Retirer"
        >
          <X size={18} />
        </button>
      </div>
    </div>
  );
}

function AddFriend() {
  const [username, setUsername] = useState("");
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null);

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    setMsg(null);
    try {
      await api.addRelationship({ username: username.trim() });
      await useStore.getState().refreshRelationships();
      setMsg({ ok: true, text: `Demande envoyée à ${username}.` });
      setUsername("");
    } catch (err) {
      setMsg({ ok: false, text: err instanceof ApiError ? err.message : "Échec." });
    }
  }

  return (
    <div className="border-b border-white/5 px-6 py-5">
      <h3 className="text-sm font-bold uppercase tracking-wide text-header">Ajouter un ami</h3>
      <p className="mb-3 mt-1 text-sm text-muted">
        Tu peux ajouter un ami avec son nom d'utilisateur.
      </p>
      <form onSubmit={submit} className="flex gap-2">
        <input
          value={username}
          onChange={(e) => setUsername(e.target.value)}
          placeholder="nom d'utilisateur"
          className="flex-1 rounded-lg bg-deepest px-3 py-2.5 text-normal outline-none ring-1 ring-transparent focus:ring-accent"
        />
        <button
          type="submit"
          disabled={!username.trim()}
          className="rounded-lg btn-accent px-5 text-sm font-medium text-white disabled:opacity-60"
        >
          Envoyer
        </button>
      </form>
      {msg && (
        <p className={`mt-2 text-sm ${msg.ok ? "text-online" : "text-dnd"}`}>{msg.text}</p>
      )}
    </div>
  );
}

function relLabel(type: RelationshipType, status?: string): string {
  if (type === "incoming") return "Demande d'ami entrante";
  if (type === "outgoing") return "Demande d'ami envoyée";
  if (type === "blocked") return "Bloqué";
  return status ?? "Hors ligne";
}

function TabBtn({
  active,
  onClick,
  children,
}: {
  active: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      onClick={onClick}
      className={`rounded-lg px-2.5 py-1 text-sm ${
        active ? "bg-selected text-header" : "text-muted hover:bg-hover hover:text-normal"
      }`}
    >
      {children}
    </button>
  );
}

function Badge({ n }: { n: number }) {
  return (
    <span className="ml-1 rounded-full bg-dnd px-1.5 text-xs font-bold text-white">{n}</span>
  );
}
