// Pages « Expression » des paramètres de serveur : Autocollants et Soundboard.
// Même langage visuel que la page Émoji (formulaire d'ajout en tête, grille en dessous).

import { useEffect, useRef, useState } from "react";
import { Pencil, Play, Trash2, Upload } from "lucide-react";
import { api } from "../api";
import { useStore } from "../store";
import { mediaUrl } from "../lib/instance";
import { staggerDelay } from "../lib/anim";
import type { SoundboardSound, Sticker } from "../types";
import { ImageCropModal } from "./ImageCropModal";
import { Spinner } from "./ui/Spinner";
import { ListSkeleton } from "./ui/Skeleton";
import {
  formatBytes,
  isGif,
  sizeError,
  STICKER_CROP_PX,
  STICKER_MAX_BYTES,
} from "../lib/imageUpload";

function errText(e: unknown): string {
  return e instanceof Error ? e.message : "Échec.";
}

// Renommage en place (nom → crayon → input, validation à Entrée/blur).
export function InlineName({
  value,
  onRename,
  className,
}: {
  value: string;
  onRename: (name: string) => Promise<void>;
  className?: string;
}) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(value);
  useEffect(() => setDraft(value), [value]);

  async function commit() {
    setEditing(false);
    const name = draft.trim();
    if (!name || name === value) {
      setDraft(value);
      return;
    }
    try {
      await onRename(name);
    } catch {
      setDraft(value);
    }
  }

  if (editing) {
    return (
      <input
        autoFocus
        value={draft}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={() => void commit()}
        onKeyDown={(e) => {
          if (e.key === "Enter") void commit();
          if (e.key === "Escape") {
            setDraft(value);
            setEditing(false);
          }
        }}
        maxLength={30}
        className="w-full min-w-0 rounded bg-deepest px-2 py-0.5 text-sm text-normal outline-none ring-1 ring-accent"
      />
    );
  }
  return (
    <button
      onClick={() => setEditing(true)}
      title="Renommer"
      className={`group/name flex min-w-0 items-center gap-1.5 text-left ${className ?? ""}`}
    >
      <span className="truncate text-sm text-normal">{value}</span>
      <Pencil size={12} className="shrink-0 text-muted opacity-0 group-hover/name:opacity-100" />
    </button>
  );
}

// ───────────────────────────── Autocollants ─────────────────────────────

export function StickersPage({ guildId }: { guildId: string }) {
  const [stickers, setStickers] = useState<Sticker[] | null>(null);
  const [name, setName] = useState("");
  // `file` = asset prêt à envoyer (recadré ou GIF original) ; `animated` mémorise le format_type.
  const [file, setFile] = useState<File | null>(null);
  const [animated, setAnimated] = useState(false);
  const [cropping, setCropping] = useState<File | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const fileInput = useRef<HTMLInputElement>(null);

  async function reload() {
    try {
      const list = await api.listStickers(guildId);
      setStickers(list);
      // Garde le cache du picker (composeur) en phase.
      useStore.setState((s) => ({ stickersByGuild: { ...s.stickersByGuild, [guildId]: list } }));
    } catch (e) {
      setError(errText(e));
    }
  }

  useEffect(() => {
    void reload();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [guildId]);

  // Sélection : vérifie la taille (2 Mio) puis recadre l'image fixe, ou conserve le GIF tel quel.
  // Toute nouvelle sélection invalide la précédente (pas d'asset obsolète soumis si crop annulé
  // ou nouvelle image rejetée).
  async function pick(f: File | null) {
    setError(null);
    setFile(null);
    setAnimated(false);
    if (fileInput.current) fileInput.current.value = "";
    if (!f) return;
    const tooBig = sizeError(f, STICKER_MAX_BYTES);
    if (tooBig) {
      setError(tooBig);
      return;
    }
    if (await isGif(f)) {
      setFile(f);
      setAnimated(true);
    } else {
      setCropping(f);
    }
  }

  function onCropped(blob: Blob) {
    setFile(new File([blob], "sticker.webp", { type: "image/webp" }));
    setAnimated(false);
    setCropping(null);
  }

  async function create() {
    if (!file || name.trim().length < 2) {
      setError("Nom (≥ 2 caractères) et image requis.");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const { image_id } = await api.uploadStickerImage(guildId, file);
      await api.createSticker(guildId, {
        name: name.trim(),
        asset_id: image_id,
        format_type: animated ? 4 : 1,
      });
      setName("");
      setFile(null);
      setAnimated(false);
      if (fileInput.current) fileInput.current.value = "";
      await reload();
    } catch (e) {
      setError(errText(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <>
    <div className="flex h-full flex-col overflow-hidden rounded-xl bg-modal ring-1 ring-cardline surface-card">
      <div className="border-b border-line px-6 py-4">
        <h2 className="text-xl font-bold text-header">Autocollants</h2>
        <p className="mt-0.5 text-sm text-muted">
          Images expressives envoyées d'un clic depuis le composeur. PNG ou GIF animé.
        </p>
      </div>

      <div className="border-b border-line bg-deepest/40 px-6 py-4">
        <div className="flex items-end gap-2">
          <div className="flex-1">
            <label className="mb-1 block text-xs font-bold uppercase tracking-wide text-subtext">
              Nom
            </label>
            <input
              value={name}
              onChange={(e) => setName(e.target.value)}
              maxLength={30}
              placeholder="mon-autocollant"
              className="w-full rounded-lg bg-deepest px-3 py-2 text-normal outline-none ring-1 ring-transparent focus:ring-accent"
            />
          </div>
          <input
            ref={fileInput}
            type="file"
            accept="image/png,image/gif,image/webp,image/jpeg"
            className="hidden"
            onChange={(e) => void pick(e.target.files?.[0] ?? null)}
          />
          <button
            onClick={() => fileInput.current?.click()}
            className="flex items-center gap-1.5 rounded bg-sidebar px-3 py-2 text-sm text-normal hover:bg-deepest"
          >
            <Upload size={16} />
            {file ? "Changer" : "Image"}
          </button>
          <button
            onClick={() => void create()}
            disabled={busy || !file || name.trim().length < 2}
            className="pressable inline-flex items-center justify-center gap-2 rounded-lg btn-accent px-4 py-2 text-sm font-medium text-white disabled:cursor-not-allowed disabled:opacity-50"
          >
            {busy && <Spinner size={14} />}Ajouter
          </button>
        </div>
        {file && (
          <p className="mt-1.5 text-xs text-muted">
            Prêt à envoyer : {formatBytes(file.size)}
            {animated ? " · GIF animé (non recadré)" : " · recadré"}
          </p>
        )}
        <p className="mt-1 text-xs text-muted/70">
          PNG, JPEG, WebP (recadrés) ou GIF animé. Max {formatBytes(STICKER_MAX_BYTES)}.
        </p>
        {error && <p className="mt-1.5 text-sm text-dnd">{error}</p>}
      </div>

      <div className="flex-1 overflow-y-auto p-4 scroll-thin">
        {stickers === null ? (
          <ListSkeleton rows={4} />
        ) : stickers.length === 0 ? (
          <p className="text-sm text-muted">Aucun autocollant. Ajoute le premier ci-dessus.</p>
        ) : (
          <div className="grid grid-cols-3 gap-2 lg:grid-cols-4">
            {stickers.map((st, i) => (
              <div key={st.id} className="animate-row-in" style={staggerDelay(i)}>
              <div className="group flex flex-col gap-2 rounded-lg bg-sidebar p-3">
                <img
                  src={mediaUrl(`/api/stickers/${st.id}`)}
                  alt={st.name}
                  className="aspect-square w-full rounded object-contain"
                  draggable={false}
                />
                <div className="flex items-center gap-1">
                  <InlineName
                    value={st.name}
                    className="flex-1"
                    onRename={(n) => api.updateSticker(guildId, st.id, { name: n }).then(reload)}
                  />
                  <button
                    onClick={() => void api.deleteSticker(guildId, st.id).then(reload).catch(() => {})}
                    title="Supprimer"
                    className="shrink-0 text-muted opacity-0 transition-opacity hover:text-dnd group-hover:opacity-100"
                  >
                    <Trash2 size={15} />
                  </button>
                </div>
              </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
    {cropping && (
      <ImageCropModal
        file={cropping}
        aspect={1}
        outWidth={STICKER_CROP_PX}
        outHeight={STICKER_CROP_PX}
        title="Recadrer l'autocollant"
        onCancel={() => setCropping(null)}
        onConfirm={onCropped}
      />
    )}
    </>
  );
}

// ───────────────────────────── Soundboard ─────────────────────────────

export function SoundboardPage({ guildId }: { guildId: string }) {
  const [sounds, setSounds] = useState<SoundboardSound[] | null>(null);
  const [name, setName] = useState("");
  const [emoji, setEmoji] = useState("");
  const [volume, setVolume] = useState(1);
  const [file, setFile] = useState<File | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const fileInput = useRef<HTMLInputElement>(null);

  async function reload() {
    try {
      const list = await api.listSounds(guildId);
      setSounds(list);
      useStore.setState((s) => ({ soundsByGuild: { ...s.soundsByGuild, [guildId]: list } }));
    } catch (e) {
      setError(errText(e));
    }
  }

  useEffect(() => {
    void reload();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [guildId]);

  async function create() {
    if (!file || name.trim().length < 2) {
      setError("Nom (≥ 2 caractères) et fichier audio requis.");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const { sound_id } = await api.uploadSoundAudio(guildId, file);
      await api.createSound(guildId, {
        name: name.trim(),
        sound_id,
        volume,
        emoji: emoji.trim() || null,
      });
      setName("");
      setEmoji("");
      setVolume(1);
      setFile(null);
      if (fileInput.current) fileInput.current.value = "";
      await reload();
    } catch (e) {
      setError(errText(e));
    } finally {
      setBusy(false);
    }
  }

  // Préécoute locale (Audio simple — rien n'est transmis dans le vocal ici).
  function preview(s: SoundboardSound) {
    const el = new Audio(mediaUrl(`/api/soundboard-sounds/${s.id}/audio`));
    el.volume = Math.min(Math.max(s.volume, 0), 1);
    void el.play().catch(() => {});
  }

  return (
    <div className="flex h-full flex-col overflow-hidden rounded-xl bg-modal ring-1 ring-cardline surface-card">
      <div className="border-b border-line px-6 py-4">
        <h2 className="text-xl font-bold text-header">Soundboard</h2>
        <p className="mt-0.5 text-sm text-muted">
          Sons courts jouables en salon vocal (MP3, OGG ou WAV — 1 Mo max).
        </p>
      </div>

      <div className="border-b border-line bg-deepest/40 px-6 py-4">
        <div className="flex items-end gap-2">
          <div className="flex-1">
            <label className="mb-1 block text-xs font-bold uppercase tracking-wide text-subtext">
              Nom
            </label>
            <input
              value={name}
              onChange={(e) => setName(e.target.value)}
              maxLength={30}
              placeholder="tada"
              className="w-full rounded-lg bg-deepest px-3 py-2 text-normal outline-none ring-1 ring-transparent focus:ring-accent"
            />
          </div>
          <div className="w-20">
            <label className="mb-1 block text-xs font-bold uppercase tracking-wide text-subtext">
              Émoji
            </label>
            <input
              value={emoji}
              onChange={(e) => setEmoji(e.target.value)}
              maxLength={8}
              placeholder="🎉"
              className="w-full rounded-lg bg-deepest px-3 py-2 text-center text-normal outline-none ring-1 ring-transparent focus:ring-accent"
            />
          </div>
          <input
            ref={fileInput}
            type="file"
            accept="audio/mpeg,audio/ogg,audio/wav,.mp3,.ogg,.wav"
            className="hidden"
            onChange={(e) => setFile(e.target.files?.[0] ?? null)}
          />
          <button
            onClick={() => fileInput.current?.click()}
            className="flex items-center gap-1.5 rounded bg-sidebar px-3 py-2 text-sm text-normal hover:bg-deepest"
          >
            <Upload size={16} />
            {file ? "Changer" : "Audio"}
          </button>
          <button
            onClick={() => void create()}
            disabled={busy || !file || name.trim().length < 2}
            className="pressable inline-flex items-center justify-center gap-2 rounded-lg btn-accent px-4 py-2 text-sm font-medium text-white disabled:cursor-not-allowed disabled:opacity-50"
          >
            {busy && <Spinner size={14} />}Ajouter
          </button>
        </div>
        <div className="mt-3 flex items-center gap-3">
          <span className="text-xs font-bold uppercase tracking-wide text-subtext">Volume</span>
          <input
            type="range"
            min={0}
            max={100}
            value={Math.round(volume * 100)}
            onChange={(e) => setVolume(Number(e.target.value) / 100)}
            className="w-48 accent-[var(--accent)]"
          />
          <span className="text-xs text-muted">{Math.round(volume * 100)} %</span>
        </div>
        {file && <p className="mt-1.5 text-xs text-muted">Fichier : {file.name}</p>}
        {error && <p className="mt-1.5 text-sm text-dnd">{error}</p>}
      </div>

      <div className="flex-1 overflow-y-auto p-4 scroll-thin">
        {sounds === null ? (
          <ListSkeleton rows={4} />
        ) : sounds.length === 0 ? (
          <p className="text-sm text-muted">Aucun son. Ajoute le premier ci-dessus.</p>
        ) : (
          <div className="flex flex-col gap-2">
            {sounds.map((s, i) => (
              <div key={s.id} className="animate-row-in" style={staggerDelay(i)}>
              <div className="group flex items-center gap-3 rounded-lg bg-sidebar px-3 py-2">
                <button
                  onClick={() => preview(s)}
                  title="Préécouter"
                  className="flex h-9 w-9 shrink-0 items-center justify-center rounded-full bg-deepest text-interactive-normal hover:text-header"
                >
                  <Play size={16} />
                </button>
                {/* Contenu utilisateur : l'émoji décoratif choisi pour le son. */}
                <span className="w-6 shrink-0 text-center text-lg">{s.emoji ?? ""}</span>
                <div className="min-w-0 flex-1">
                  <InlineName
                    value={s.name}
                    onRename={(n) => api.updateSound(guildId, s.id, { name: n }).then(reload)}
                  />
                </div>
                <input
                  type="range"
                  min={0}
                  max={100}
                  defaultValue={Math.round(s.volume * 100)}
                  title="Volume"
                  onMouseUp={(e) =>
                    void api
                      .updateSound(guildId, s.id, {
                        volume: Number((e.target as HTMLInputElement).value) / 100,
                      })
                      .then(reload)
                      .catch(() => {})
                  }
                  className="w-28 accent-[var(--accent)]"
                />
                <button
                  onClick={() => void api.deleteSound(guildId, s.id).then(reload).catch(() => {})}
                  title="Supprimer"
                  className="shrink-0 text-muted opacity-0 transition-opacity hover:text-dnd group-hover:opacity-100"
                >
                  <Trash2 size={15} />
                </button>
              </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
