import { useEffect, useRef, useState } from "react";
import { Trash2, Upload } from "lucide-react";
import { api } from "../api";
import type { Emoji } from "../types";
import { Modal } from "./ServerRail";
import { InlineName } from "./ExpressionPages";
import { ImageCropModal } from "./ImageCropModal";
import { Spinner } from "./ui/Spinner";
import { EMOJI_CROP_PX, EMOJI_MAX_BYTES, formatBytes, isGif, sizeError } from "../lib/imageUpload";

// Gestion des emoji personnalisés d'une guilde (lister / téléverser / supprimer).
export function EmojiModal({
  guildId,
  onClose,
  embedded,
}: {
  guildId: string;
  onClose?: () => void;
  embedded?: boolean;
}) {
  const [emojis, setEmojis] = useState<Emoji[] | null>(null);
  const [name, setName] = useState("");
  // `file` = ce qui sera envoyé (image recadrée ou GIF original). `cropping` = source en attente
  // de recadrage (image fixe). `animated` mémorise si la source était un GIF (recadrage sauté).
  const [file, setFile] = useState<File | null>(null);
  const [animated, setAnimated] = useState(false);
  const [cropping, setCropping] = useState<File | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const fileInput = useRef<HTMLInputElement>(null);

  async function reload() {
    try {
      setEmojis(await api.listEmojis(guildId));
    } catch (e) {
      setError(e instanceof Error ? e.message : "Erreur.");
    }
  }

  useEffect(() => {
    void reload();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [guildId]);

  // Sélection d'un fichier : vérifie la taille, puis recadre (image fixe) ou conserve tel quel (GIF).
  // Toute nouvelle sélection invalide la précédente (évite de soumettre un asset obsolète si le
  // recadrage est annulé ou si la nouvelle image est rejetée).
  async function pick(f: File | null) {
    setError(null);
    setFile(null);
    setAnimated(false);
    if (fileInput.current) fileInput.current.value = "";
    if (!f) return;
    const tooBig = sizeError(f, EMOJI_MAX_BYTES);
    if (tooBig) {
      setError(tooBig);
      return;
    }
    if (await isGif(f)) {
      // GIF animé : pas de recadrage (le canvas aplatirait l'animation) — envoyé tel quel.
      setFile(f);
      setAnimated(true);
    } else {
      // Image fixe : on ouvre l'assistant de recadrage.
      setCropping(f);
    }
  }

  // Recadrage validé : on récupère un blob WebP carré qu'on traite comme le fichier à envoyer.
  function onCropped(blob: Blob) {
    setFile(new File([blob], "emoji.webp", { type: "image/webp" }));
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
      const { image_id } = await api.uploadEmojiImage(guildId, file);
      await api.createEmoji(guildId, {
        name: name.trim(),
        image_id,
        animated,
      });
      setName("");
      setFile(null);
      setAnimated(false);
      if (fileInput.current) fileInput.current.value = "";
      await reload();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Échec.");
    } finally {
      setBusy(false);
    }
  }

  const content = (
      <div
        className={`flex flex-col overflow-hidden rounded-xl bg-modal ring-1 ring-cardline surface-card ${
          embedded ? "h-full w-full" : "h-[520px] w-[560px]"
        }`}
      >
        <div className="border-b border-line px-6 py-4">
          <h2 className="text-xl font-bold text-header">Emojis</h2>
        </div>

        <div className="border-b border-line bg-deepest/40 px-6 py-4">
          <div className="flex items-end gap-2">
            <div className="flex-1">
              <label className="mb-1 block text-xs font-bold uppercase tracking-wide text-subtext">
                Nom (lettres, chiffres, _)
              </label>
              <input
                value={name}
                onChange={(e) => setName(e.target.value.replace(/[^A-Za-z0-9_]/g, ""))}
                maxLength={32}
                placeholder="mon_emoji"
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
            PNG, JPEG, WebP (recadrés) ou GIF animé. Max {formatBytes(EMOJI_MAX_BYTES)}.
          </p>
          {error && <p className="mt-1.5 text-sm text-dnd">{error}</p>}
        </div>

        <div className="flex-1 overflow-y-auto p-4 scroll-thin">
          {emojis === null ? (
            <p className="text-sm text-muted">Chargement…</p>
          ) : emojis.length === 0 ? (
            <p className="text-sm text-muted">Aucun emoji. Ajoute le premier ci-dessus.</p>
          ) : (
            <div className="grid grid-cols-2 gap-2">
              {emojis.map((e) => (
                <div key={e.id} className="flex items-center gap-3 rounded bg-sidebar px-3 py-2">
                  <img src={`/api/emojis/${e.id}`} alt={e.name} className="h-8 w-8 object-contain" />
                  <div className="min-w-0 flex-1">
                    <InlineName
                      value={e.name}
                      onRename={(n) =>
                        api
                          .updateEmoji(guildId, e.id, { name: n.replace(/[^A-Za-z0-9_]/g, "") })
                          .then(reload)
                      }
                    />
                  </div>
                  <button
                    onClick={() => void api.deleteEmoji(guildId, e.id).then(reload).catch(() => {})}
                    title="Supprimer"
                    className="text-muted hover:text-dnd"
                  >
                    <Trash2 size={16} />
                  </button>
                </div>
              ))}
            </div>
          )}
        </div>

        {!embedded && (
          <div className="flex justify-end border-t border-line px-6 py-3">
            <button onClick={onClose} className="px-4 py-2 text-sm text-normal hover:underline">
              Fermer
            </button>
          </div>
        )}
      </div>
  );
  return (
    <>
      {embedded ? content : <Modal onClose={onClose ?? (() => {})}>{content}</Modal>}
      {cropping && (
        <ImageCropModal
          file={cropping}
          aspect={1}
          outWidth={EMOJI_CROP_PX}
          outHeight={EMOJI_CROP_PX}
          title="Recadrer l'emoji"
          onCancel={() => setCropping(null)}
          onConfirm={onCropped}
        />
      )}
    </>
  );
}
