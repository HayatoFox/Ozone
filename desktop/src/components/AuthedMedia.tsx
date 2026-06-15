import { useEffect, useRef, useState } from "react";
import { Download, ImageOff, Maximize2, Pause, Play, Volume2, VolumeX } from "lucide-react";
import { getAccessToken } from "../api";
import { appFetch } from "../lib/instance";

// Récupère une ressource protégée (bearer) en blob et expose une URL d'objet locale.
// Nécessaire car les requêtes <img>/<a> du navigateur ne portent pas l'en-tête Authorization.
function useAuthedBlob(src: string): { url: string | null; failed: boolean } {
  const [url, setUrl] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);
  useEffect(() => {
    let cancelled = false;
    let obj: string | null = null;
    setUrl(null);
    setFailed(false);
    (async () => {
      try {
        const token = getAccessToken();
        const res = await appFetch(src, {
          headers: token ? { Authorization: `Bearer ${token}` } : {},
        });
        if (!res.ok) {
          if (!cancelled) setFailed(true);
          return;
        }
        const blob = await res.blob();
        if (cancelled) return;
        obj = URL.createObjectURL(blob);
        setUrl(obj);
      } catch {
        if (!cancelled) setFailed(true);
      }
    })();
    return () => {
      cancelled = true;
      if (obj) URL.revokeObjectURL(obj);
    };
  }, [src]);
  return { url, failed };
}

// Image protégée par bearer (pièces jointes). Rend un placeholder tant que le blob charge.
export function AuthedImage({
  src,
  alt,
  className,
  onClick,
}: {
  src: string;
  alt: string;
  className?: string;
  onClick?: () => void;
}) {
  const { url, failed } = useAuthedBlob(src);
  if (failed) {
    return (
      <div
        className={`${className ?? ""} flex items-center justify-center bg-black/20 text-muted`}
        title="Média indisponible"
      >
        <ImageOff size={20} />
      </div>
    );
  }
  if (!url) {
    return <div className={`${className ?? ""} animate-pulse bg-black/20`} aria-busy />;
  }
  return <img src={url} alt={alt} className={className} onClick={onClick} loading="lazy" />;
}

// mm:ss (ou h:mm:ss au-delà d'une heure).
function fmtTime(s: number): string {
  if (!isFinite(s) || s < 0) return "0:00";
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = Math.floor(s % 60);
  const mm = h > 0 ? m.toString().padStart(2, "0") : `${m}`;
  return `${h > 0 ? `${h}:` : ""}${mm}:${sec.toString().padStart(2, "0")}`;
}

// Lecteur vidéo personnalisé : contrôles épurés (lecture, temps, barre de progression), barre de
// volume VERTICALE qui s'ouvre au survol de l'icône, bouton de téléchargement dans le coin, plein écran.
function VideoPlayer({
  url,
  className,
  filename,
}: {
  url: string;
  className?: string;
  filename?: string;
}) {
  const wrapRef = useRef<HTMLDivElement>(null);
  const vidRef = useRef<HTMLVideoElement>(null);
  const [playing, setPlaying] = useState(false);
  const [cur, setCur] = useState(0);
  const [dur, setDur] = useState(0);
  const [volume, setVolume] = useState(1);
  const [muted, setMuted] = useState(false);
  const [volOpen, setVolOpen] = useState(false);

  const togglePlay = () => {
    const v = vidRef.current;
    if (!v) return;
    if (v.paused) void v.play();
    else v.pause();
  };
  const seek = (t: number) => {
    const v = vidRef.current;
    if (v) v.currentTime = t;
    setCur(t);
  };
  const applyVolume = (val: number) => {
    const v = vidRef.current;
    setVolume(val);
    setMuted(val === 0);
    if (v) {
      v.volume = val;
      v.muted = val === 0;
    }
  };
  const toggleMute = () => {
    const v = vidRef.current;
    if (!v) return;
    const next = !v.muted;
    v.muted = next;
    setMuted(next);
    if (!next && v.volume === 0) applyVolume(0.5);
  };
  const fullscreen = () => {
    void wrapRef.current?.requestFullscreen?.();
  };

  return (
    <div
      ref={wrapRef}
      className={`group/vid relative overflow-hidden rounded-md bg-black ${className ?? ""}`}
    >
      <video
        ref={vidRef}
        src={url}
        preload="metadata"
        className="h-full w-full"
        onClick={togglePlay}
        onPlay={() => setPlaying(true)}
        onPause={() => setPlaying(false)}
        onTimeUpdate={(e) => setCur(e.currentTarget.currentTime)}
        onLoadedMetadata={(e) => setDur(e.currentTarget.duration)}
        onVolumeChange={(e) => {
          setVolume(e.currentTarget.volume);
          setMuted(e.currentTarget.muted);
        }}
      />

      {/* Téléchargement, coin haut-droit (apparaît au survol). Blob déjà en mémoire → lien direct. */}
      <a
        href={url}
        download={filename || "video"}
        onClick={(e) => e.stopPropagation()}
        title="Télécharger"
        className="absolute right-2 top-2 flex h-8 w-8 items-center justify-center rounded-md bg-black/55 text-white opacity-0 backdrop-blur-sm transition hover:bg-black/75 group-hover/vid:opacity-100"
      >
        <Download size={16} />
      </a>

      {/* Gros bouton lecture au centre quand en pause. */}
      {!playing && (
        <button
          onClick={togglePlay}
          className="absolute inset-0 flex items-center justify-center"
          title="Lecture"
        >
          <span className="flex h-14 w-14 items-center justify-center rounded-full bg-black/55 text-white backdrop-blur-sm transition hover:scale-105 hover:bg-black/70">
            <Play size={26} className="ml-0.5" fill="currentColor" />
          </span>
        </button>
      )}

      {/* Barre de contrôle bas (apparaît au survol). */}
      <div className="absolute inset-x-0 bottom-0 flex items-center gap-2 bg-gradient-to-t from-black/70 to-transparent px-2 pb-1.5 pt-6 opacity-0 transition group-hover/vid:opacity-100">
        <button onClick={togglePlay} className="text-white/90 hover:text-white" title={playing ? "Pause" : "Lecture"}>
          {playing ? <Pause size={18} fill="currentColor" /> : <Play size={18} fill="currentColor" />}
        </button>
        <span className="select-none text-xs tabular-nums text-white/85">
          {fmtTime(cur)} / {fmtTime(dur)}
        </span>
        {/* Barre de progression. */}
        <input
          type="range"
          min={0}
          max={dur || 0}
          step="any"
          value={Math.min(cur, dur || 0)}
          onChange={(e) => seek(Number(e.target.value))}
          onClick={(e) => e.stopPropagation()}
          className="h-1 flex-1 cursor-pointer accent-accent"
          title="Position"
        />
        {/* Volume : icône + barre VERTICALE au survol. */}
        <div
          className="relative flex items-center"
          onMouseEnter={() => setVolOpen(true)}
          onMouseLeave={() => setVolOpen(false)}
        >
          {volOpen && (
            <div className="absolute bottom-7 left-1/2 -translate-x-1/2 rounded-md bg-black/80 px-2 py-2 backdrop-blur-sm">
              <input
                type="range"
                min={0}
                max={1}
                step={0.05}
                value={muted ? 0 : volume}
                onChange={(e) => applyVolume(Number(e.target.value))}
                className="h-20 w-1 cursor-pointer accent-accent"
                style={{ writingMode: "vertical-lr", direction: "rtl" }}
                title="Volume"
              />
            </div>
          )}
          <button onClick={toggleMute} className="text-white/90 hover:text-white" title="Son">
            {muted || volume === 0 ? <VolumeX size={18} /> : <Volume2 size={18} />}
          </button>
        </div>
        <button onClick={fullscreen} className="text-white/90 hover:text-white" title="Plein écran">
          <Maximize2 size={16} />
        </button>
      </div>
    </div>
  );
}

// Vidéo protégée par bearer : récupère le blob puis l'expose dans le lecteur personnalisé.
// (Le navigateur ne porte pas l'Authorization sur un <video src> distant → on passe par un blob.)
export function AuthedVideo({
  src,
  className,
  filename,
}: {
  src: string;
  className?: string;
  filename?: string;
}) {
  const { url, failed } = useAuthedBlob(src);
  if (failed) {
    return (
      <div
        className={`${className ?? ""} flex items-center justify-center bg-black/20 text-muted`}
        title="Vidéo indisponible"
      >
        <ImageOff size={20} />
      </div>
    );
  }
  if (!url) {
    return <div className={`${className ?? ""} animate-pulse bg-black/20`} aria-busy />;
  }
  return <VideoPlayer url={url} className={className} filename={filename} />;
}

// Audio protégé par bearer : lecteur <audio> natif sur blob.
export function AuthedAudio({ src, className }: { src: string; className?: string }) {
  const { url, failed } = useAuthedBlob(src);
  if (failed || !url) {
    return <div className={`${className ?? ""} h-8 animate-pulse rounded bg-black/20`} aria-busy />;
  }
  return <audio src={url} controls preload="metadata" className={className} />;
}

// Télécharge une ressource protégée (bearer) puis déclenche l'enregistrement local.
export async function authedDownload(src: string, filename: string): Promise<void> {
  const token = getAccessToken();
  const res = await appFetch(src, { headers: token ? { Authorization: `Bearer ${token}` } : {} });
  if (!res.ok) return;
  const blob = await res.blob();
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  a.remove();
  URL.revokeObjectURL(url);
}
