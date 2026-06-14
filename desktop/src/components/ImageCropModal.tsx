import { useEffect, useRef, useState } from "react";
import { ZoomIn, ZoomOut } from "lucide-react";
import { Modal } from "./ServerRail";
import { Spinner } from "./ui/Spinner";

// Assistant de recadrage/redimensionnement (pan + zoom) — sans dépendance externe.
// L'utilisateur déplace/zoome l'image dans un cadre ; à la validation on exporte la région
// visible vers un canvas à la taille de sortie voulue (icône carrée, bannière large).
export function ImageCropModal({
  file,
  aspect,
  outWidth,
  outHeight,
  round,
  title,
  onCancel,
  onConfirm,
}: {
  file: File;
  aspect: number; // largeur / hauteur du cadre
  outWidth: number;
  outHeight: number;
  round?: boolean;
  title: string;
  onCancel: () => void;
  onConfirm: (blob: Blob) => void;
}) {
  const FRAME_W = 360;
  const FRAME_H = Math.round(FRAME_W / aspect);

  const [img, setImg] = useState<HTMLImageElement | null>(null);
  const [scale, setScale] = useState(1);
  const [minScale, setMinScale] = useState(1);
  const [offset, setOffset] = useState({ x: 0, y: 0 });
  const [dragging, setDragging] = useState(false);
  const [busy, setBusy] = useState(false);
  const drag = useRef<{ x: number; y: number; ox: number; oy: number } | null>(null);

  useEffect(() => {
    const url = URL.createObjectURL(file);
    const i = new Image();
    i.onload = () => {
      const cover = Math.max(FRAME_W / i.naturalWidth, FRAME_H / i.naturalHeight);
      setMinScale(cover);
      setScale(cover);
      setOffset({
        x: (FRAME_W - i.naturalWidth * cover) / 2,
        y: (FRAME_H - i.naturalHeight * cover) / 2,
      });
      setImg(i);
    };
    i.src = url;
    return () => URL.revokeObjectURL(url);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [file]);

  // Garde l'image couvrant toujours le cadre (jamais de vide sur les bords).
  function clamp(o: { x: number; y: number }, s: number, i = img) {
    if (!i) return o;
    const w = i.naturalWidth * s;
    const h = i.naturalHeight * s;
    return {
      x: Math.min(0, Math.max(FRAME_W - w, o.x)),
      y: Math.min(0, Math.max(FRAME_H - h, o.y)),
    };
  }

  function applyScale(next: number) {
    if (!img) return;
    const s = Math.max(minScale, Math.min(minScale * 5, next));
    const cx = FRAME_W / 2;
    const cy = FRAME_H / 2;
    const k = s / scale;
    const nx = cx - (cx - offset.x) * k;
    const ny = cy - (cy - offset.y) * k;
    setOffset(clamp({ x: nx, y: ny }, s));
    setScale(s);
  }

  async function confirm() {
    if (!img) return;
    setBusy(true);
    const canvas = document.createElement("canvas");
    canvas.width = outWidth;
    canvas.height = outHeight;
    const ctx = canvas.getContext("2d");
    if (!ctx) {
      setBusy(false);
      return;
    }
    const sx = -offset.x / scale;
    const sy = -offset.y / scale;
    const sw = FRAME_W / scale;
    const sh = FRAME_H / scale;
    ctx.drawImage(img, sx, sy, sw, sh, 0, 0, outWidth, outHeight);
    canvas.toBlob(
      (blob) => {
        setBusy(false);
        if (blob) onConfirm(blob);
      },
      "image/webp",
      0.9,
    );
  }

  return (
    <Modal onClose={onCancel}>
      <div className="w-[420px] rounded-xl border border-line bg-modal p-6 shadow-2xl">
        <h3 className="mb-4 text-lg font-bold text-header">{title}</h3>
        <div
          className="relative mx-auto touch-none select-none overflow-hidden bg-black/60"
          style={{
            width: FRAME_W,
            height: FRAME_H,
            borderRadius: round ? "9999px" : "14px",
            cursor: dragging ? "grabbing" : "grab",
          }}
          onPointerDown={(e) => {
            (e.target as HTMLElement).setPointerCapture?.(e.pointerId);
            drag.current = { x: e.clientX, y: e.clientY, ox: offset.x, oy: offset.y };
            setDragging(true);
          }}
          onPointerMove={(e) => {
            if (!drag.current) return;
            setOffset(
              clamp(
                {
                  x: drag.current.ox + (e.clientX - drag.current.x),
                  y: drag.current.oy + (e.clientY - drag.current.y),
                },
                scale,
              ),
            );
          }}
          onPointerUp={() => {
            drag.current = null;
            setDragging(false);
          }}
          onWheel={(e) => {
            e.preventDefault();
            applyScale(scale * (e.deltaY < 0 ? 1.1 : 0.9));
          }}
        >
          {img && (
            <img
              src={img.src}
              alt=""
              draggable={false}
              style={{
                position: "absolute",
                left: offset.x,
                top: offset.y,
                width: img.naturalWidth * scale,
                height: img.naturalHeight * scale,
                maxWidth: "none",
              }}
            />
          )}
          {/* Voile + grille de tiers pour guider le cadrage. */}
          <div className="pointer-events-none absolute inset-0 ring-1 ring-inset ring-white/20" />
        </div>

        <div className="mt-4 flex items-center gap-3">
          <ZoomOut size={16} className="shrink-0 text-muted" />
          <input
            type="range"
            min={minScale}
            max={minScale * 5}
            step={0.0001}
            value={scale}
            onChange={(e) => applyScale(parseFloat(e.target.value))}
            className="flex-1 accent-accent"
          />
          <ZoomIn size={16} className="shrink-0 text-muted" />
        </div>
        <p className="mt-2 text-center text-xs text-muted">Glisse pour repositionner · molette ou curseur pour zoomer</p>

        <div className="mt-5 flex justify-end gap-3">
          <button onClick={onCancel} className="px-4 py-2 text-sm text-normal hover:underline">
            Annuler
          </button>
          <button
            onClick={() => void confirm()}
            disabled={busy || !img}
            className="pressable inline-flex items-center justify-center gap-2 rounded-lg btn-accent px-5 py-2 text-sm font-semibold text-white disabled:cursor-not-allowed disabled:opacity-50"
          >
            {busy && <Spinner size={14} />}Appliquer
          </button>
        </div>
      </div>
    </Modal>
  );
}
