import { useEffect, useState } from "react";
import { CalendarPlus, Check, Pencil, Play, Square, Trash2, X } from "lucide-react";
import { api } from "../api";
import type { ScheduledEvent } from "../types";
import { Modal } from "./ServerRail";
import { Spinner } from "./ui/Spinner";

const STATUS: Record<number, string> = { 1: "Programmé", 2: "En cours", 3: "Terminé", 4: "Annulé" };

export function EventsModal({ guildId, onClose }: { guildId: string; onClose: () => void }) {
  const [events, setEvents] = useState<ScheduledEvent[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);
  // Événement en cours d'édition (le formulaire de création est alors pré-rempli).
  const [editing, setEditing] = useState<ScheduledEvent | null>(null);
  const [rsvped, setRsvped] = useState<Set<string>>(new Set());

  async function reload() {
    try {
      setEvents(await api.listEvents(guildId));
    } catch (e) {
      setError(e instanceof Error ? e.message : "Erreur.");
    }
  }

  useEffect(() => {
    void reload();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [guildId]);

  async function setStatus(ev: ScheduledEvent, status: number) {
    try {
      await api.updateEvent(guildId, ev.id, { status });
      await reload();
    } catch {
      /* ignore */
    }
  }

  async function toggleRsvp(ev: ScheduledEvent) {
    const has = rsvped.has(ev.id);
    try {
      if (has) await api.unrsvpEvent(guildId, ev.id);
      else await api.rsvpEvent(guildId, ev.id);
      setRsvped((s) => {
        const n = new Set(s);
        if (has) n.delete(ev.id);
        else n.add(ev.id);
        return n;
      });
      await reload();
    } catch {
      /* ignore */
    }
  }

  return (
    <Modal onClose={onClose}>
      <div className="flex h-[560px] w-[620px] flex-col overflow-hidden rounded-xl bg-modal ring-1 ring-cardline surface-card">
        <div className="flex items-center justify-between border-b border-line px-6 py-4">
          <h2 className="text-xl font-bold text-header">Événements</h2>
          <button
            onClick={() => setCreating((v) => !v)}
            className="flex items-center gap-1.5 rounded-lg btn-accent px-3 py-1.5 text-sm font-medium text-white"
          >
            <CalendarPlus size={16} />
            Créer
          </button>
        </div>

        {(creating || editing) && (
          <EventForm
            guildId={guildId}
            event={editing}
            onDone={async () => {
              setCreating(false);
              setEditing(null);
              await reload();
            }}
          />
        )}

        <div className="flex-1 overflow-y-auto p-3 scroll-thin">
          {error ? (
            <p className="p-3 text-sm text-dnd">{error}</p>
          ) : events === null ? (
            <p className="p-3 text-sm text-muted">Chargement…</p>
          ) : events.length === 0 ? (
            <p className="p-3 text-sm text-muted">Aucun événement programmé.</p>
          ) : (
            events.map((ev) => (
              <div key={ev.id} className="mb-2 rounded-lg bg-sidebar p-4">
                <div className="flex items-start justify-between">
                  <div className="min-w-0">
                    <div className="text-xs font-semibold uppercase text-online">
                      {new Date(ev.scheduled_start).toLocaleString("fr-FR", {
                        weekday: "short",
                        day: "numeric",
                        month: "short",
                        hour: "2-digit",
                        minute: "2-digit",
                      })}{" "}
                      · {STATUS[ev.status] ?? ""}
                    </div>
                    <div className="mt-0.5 font-bold text-header">{ev.name}</div>
                    {ev.location && <div className="text-sm text-muted">{ev.location}</div>}
                    {ev.description && (
                      <p className="mt-1 whitespace-pre-wrap break-words text-sm text-normal">
                        {ev.description}
                      </p>
                    )}
                    <div className="mt-1 text-xs text-muted">{ev.interested_count} intéressé(s)</div>
                  </div>
                  <div className="flex gap-2">
                    {ev.status < 3 && (
                      <button
                        onClick={() => {
                          setCreating(false);
                          setEditing(ev);
                        }}
                        title="Modifier"
                        className="text-muted hover:text-normal"
                      >
                        <Pencil size={16} />
                      </button>
                    )}
                    <button
                      onClick={() => void api.deleteEvent(guildId, ev.id).then(reload).catch(() => {})}
                      title="Supprimer"
                      className="text-muted hover:text-dnd"
                    >
                      <Trash2 size={16} />
                    </button>
                  </div>
                </div>
                <div className="mt-3 flex flex-wrap items-center gap-2">
                  <button
                    onClick={() => void toggleRsvp(ev)}
                    className={`flex items-center gap-1.5 rounded px-4 py-1.5 text-sm font-medium ${
                      rsvped.has(ev.id)
                        ? "bg-online text-white"
                        : "bg-deepest text-normal hover:bg-black/40"
                    }`}
                  >
                    {rsvped.has(ev.id) && <Check size={15} />}
                    {rsvped.has(ev.id) ? "Intéressé" : "Ça m'intéresse"}
                  </button>
                  {/* Transitions de statut : démarrer (1→2), terminer (2→3), annuler (1→4). */}
                  {ev.status === 1 && (
                    <button
                      onClick={() => void setStatus(ev, 2)}
                      className="flex items-center gap-1.5 rounded bg-deepest px-3 py-1.5 text-sm text-online hover:bg-black/40"
                    >
                      <Play size={14} />
                      Démarrer
                    </button>
                  )}
                  {ev.status === 2 && (
                    <button
                      onClick={() => void setStatus(ev, 3)}
                      className="flex items-center gap-1.5 rounded bg-deepest px-3 py-1.5 text-sm text-normal hover:bg-black/40"
                    >
                      <Square size={13} />
                      Terminer
                    </button>
                  )}
                  {ev.status === 1 && (
                    <button
                      onClick={() => void setStatus(ev, 4)}
                      className="flex items-center gap-1.5 rounded bg-deepest px-3 py-1.5 text-sm text-muted hover:bg-black/40 hover:text-dnd"
                    >
                      <X size={14} />
                      Annuler
                    </button>
                  )}
                </div>
              </div>
            ))
          )}
        </div>

        <div className="flex justify-end border-t border-line px-6 py-3">
          <button onClick={onClose} className="px-4 py-2 text-sm text-normal hover:underline">
            Fermer
          </button>
        </div>
      </div>
    </Modal>
  );
}

// Format datetime-local (heure locale, sans secondes) pour pré-remplir l'édition.
function toLocalInput(ms: number): string {
  const d = new Date(ms);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

// Formulaire de création OU d'édition (pré-rempli quand `event` est fourni).
function EventForm({
  guildId,
  event,
  onDone,
}: {
  guildId: string;
  event: ScheduledEvent | null;
  onDone: () => void | Promise<void>;
}) {
  const [name, setName] = useState(event?.name ?? "");
  const [location, setLocation] = useState(event?.location ?? "");
  const [description, setDescription] = useState(event?.description ?? "");
  const [start, setStart] = useState(event ? toLocalInput(event.scheduled_start) : "");
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  // Resynchronise le formulaire quand on change d'événement à éditer.
  useEffect(() => {
    setName(event?.name ?? "");
    setLocation(event?.location ?? "");
    setDescription(event?.description ?? "");
    setStart(event ? toLocalInput(event.scheduled_start) : "");
    setErr(null);
  }, [event]);

  async function submit() {
    const ms = start ? new Date(start).getTime() : 0;
    if (!name.trim() || !ms) {
      setErr("Nom et date requis.");
      return;
    }
    setBusy(true);
    setErr(null);
    try {
      if (event) {
        await api.updateEvent(guildId, event.id, {
          name: name.trim(),
          description: description.trim() || null,
          location: location.trim() || null,
          scheduled_start: ms,
        });
      } else {
        await api.createEvent(guildId, {
          name: name.trim(),
          description: description.trim() || null,
          entity_type: 3, // externe (lieu)
          location: location.trim() || null,
          scheduled_start: ms,
        });
      }
      await onDone();
    } catch (e) {
      setErr(e instanceof Error ? e.message : "Échec.");
      setBusy(false);
    }
  }

  return (
    <div className="space-y-2 border-b border-line bg-deepest/40 px-6 py-4">
      <input
        placeholder="Nom de l'événement"
        value={name}
        onChange={(e) => setName(e.target.value)}
        className="w-full rounded-lg bg-deepest px-3 py-2 text-normal outline-none ring-1 ring-transparent focus:ring-accent"
      />
      <div className="flex gap-2">
        <input
          type="datetime-local"
          value={start}
          onChange={(e) => setStart(e.target.value)}
          className="flex-1 rounded-lg bg-deepest px-3 py-2 text-normal outline-none ring-1 ring-transparent focus:ring-accent"
        />
        <input
          placeholder="Lieu"
          value={location}
          onChange={(e) => setLocation(e.target.value)}
          className="flex-1 rounded-lg bg-deepest px-3 py-2 text-normal outline-none ring-1 ring-transparent focus:ring-accent"
        />
      </div>
      <textarea
        placeholder="Description (optionnel)"
        value={description}
        onChange={(e) => setDescription(e.target.value)}
        rows={2}
        className="w-full resize-none rounded-lg bg-deepest px-3 py-2 text-normal outline-none ring-1 ring-transparent focus:ring-accent"
      />
      {err && <p className="text-sm text-dnd">{err}</p>}
      <div className="flex justify-end">
        <button
          onClick={() => void submit()}
          disabled={busy}
          className="pressable inline-flex items-center justify-center gap-2 rounded-lg btn-accent px-4 py-1.5 text-sm font-medium text-white disabled:cursor-not-allowed disabled:opacity-60"
        >
          {busy && <Spinner size={14} />}{event ? "Enregistrer" : "Créer l'événement"}
        </button>
      </div>
    </div>
  );
}
