import { useEffect, useState } from "react";
import { ImageOff } from "lucide-react";
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

// Vidéo protégée par bearer : récupère le blob puis l'expose dans un lecteur <video> natif.
// (Le navigateur ne porte pas l'Authorization sur un <video src> distant → on passe par un blob.)
export function AuthedVideo({ src, className }: { src: string; className?: string }) {
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
  return <video src={url} controls preload="metadata" className={className} />;
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
