// Préférences de périphériques média (micro / caméra / sortie) + traitement du son.
// Persistées en localStorage et consommées par lib/voice.ts à chaque acquisition de flux.

// Raccourci clavier : code de touche + modificateurs requis. Sérialisable (persisté en localStorage).
export interface KeyCombo {
  code: string; // KeyboardEvent.code (ex. "KeyM")
  ctrl: boolean;
  shift: boolean;
  alt: boolean;
}

export interface MediaPrefs {
  micId: string | null; // deviceId du micro (null = défaut système)
  camId: string | null; // deviceId de la caméra
  outputId: string | null; // deviceId de la sortie audio (setSinkId)
  echoCancellation: boolean;
  noiseSuppression: boolean;
  autoGainControl: boolean;
  // Mode d'entrée vocale : détection de la voix (défaut) ou appuyer-pour-parler.
  inputMode: "voice" | "ptt";
  pttKey: string; // KeyboardEvent.code de la touche maintenue (mode ptt)
  // Sensibilité de détection de la voix : seuil RMS manuel (0..1) ou calibrage automatique.
  vadAuto: boolean; // true = seuil calibré automatiquement sur le bruit de fond
  vadThreshold: number; // seuil RMS manuel quand vadAuto = false (0..1)
  // Raccourcis (fenêtre au premier plan) — rebindables.
  muteKey: KeyCombo; // bascule micro
  deafenKey: KeyCombo; // bascule sourdine
  // Partage d'écran : résolution cible (hauteur en px ; 0 = « source », sans contrainte),
  // images par seconde cibles, et partage (ou non) du son de la source.
  streamHeight: StreamHeight;
  streamFps: StreamFps;
  streamAudio: boolean;
}

// Hauteur cible du partage d'écran. 0 ⇒ « Source » (résolution native, sans contrainte).
export type StreamHeight = 0 | 720 | 1080 | 1440;
export type StreamFps = 15 | 30 | 60;

export const DEFAULT_MEDIA_PREFS: MediaPrefs = {
  micId: null,
  camId: null,
  outputId: null,
  echoCancellation: true,
  noiseSuppression: true,
  autoGainControl: true,
  inputMode: "voice",
  pttKey: "Backquote",
  vadAuto: true,
  vadThreshold: 0.045,
  muteKey: { code: "KeyM", ctrl: true, shift: true, alt: false },
  deafenKey: { code: "KeyD", ctrl: true, shift: true, alt: false },
  streamHeight: 1080,
  streamFps: 30,
  streamAudio: true,
};

/** Vrai si un évènement clavier correspond exactement au combo (mêmes modificateurs + code). */
export function comboMatches(e: KeyboardEvent, c: KeyCombo): boolean {
  return (
    e.code === c.code &&
    e.ctrlKey === c.ctrl &&
    e.shiftKey === c.shift &&
    e.altKey === c.alt
  );
}

/** Libellé lisible d'un combo (ex. « Ctrl + Maj + M »). */
export function comboLabel(c: KeyCombo): string {
  const parts: string[] = [];
  if (c.ctrl) parts.push("Ctrl");
  if (c.shift) parts.push("Maj");
  if (c.alt) parts.push("Alt");
  const key = c.code.startsWith("Key")
    ? c.code.slice(3)
    : c.code.startsWith("Digit")
      ? c.code.slice(5)
      : c.code;
  parts.push(key);
  return parts.join(" + ");
}

const KEY = "ozone.media";

export function loadMediaPrefs(): MediaPrefs {
  if (typeof localStorage === "undefined") return DEFAULT_MEDIA_PREFS;
  try {
    const raw = localStorage.getItem(KEY);
    if (!raw) return DEFAULT_MEDIA_PREFS;
    return { ...DEFAULT_MEDIA_PREFS, ...(JSON.parse(raw) as Partial<MediaPrefs>) };
  } catch {
    return DEFAULT_MEDIA_PREFS;
  }
}

export function saveMediaPrefs(p: MediaPrefs): void {
  if (typeof localStorage === "undefined") return;
  localStorage.setItem(KEY, JSON.stringify(p));
}

/** Contraintes micro : traitement selon les préférences + périphérique choisi. */
export function audioConstraints(): MediaTrackConstraints {
  const p = loadMediaPrefs();
  return {
    echoCancellation: p.echoCancellation,
    noiseSuppression: p.noiseSuppression,
    autoGainControl: p.autoGainControl,
    channelCount: 1,
    ...(p.micId ? { deviceId: { ideal: p.micId } } : {}),
  };
}

/** Contraintes caméra : 720p30 + périphérique choisi. */
export function videoConstraints(): MediaTrackConstraints {
  const p = loadMediaPrefs();
  return {
    width: 1280,
    height: 720,
    frameRate: 30,
    ...(p.camId ? { deviceId: { ideal: p.camId } } : {}),
  };
}

/**
 * Contraintes vidéo du partage d'écran selon les préférences (hauteur + fps). `height = 0`
 * (« Source ») ⇒ aucune contrainte de taille (résolution native). Les contraintes sont en `ideal`
 * (jamais `exact`) pour que le navigateur dégrade proprement plutôt que d'échouer.
 */
export function screenVideoConstraints(p: MediaPrefs): MediaTrackConstraints {
  const c: MediaTrackConstraints = { frameRate: { ideal: p.streamFps } };
  if (p.streamHeight > 0) {
    c.height = { ideal: p.streamHeight };
    // 16:9 indicatif (le navigateur ajuste à la source) pour viser ~1080p/1440p larges.
    c.width = { ideal: Math.round((p.streamHeight * 16) / 9) };
  }
  return c;
}

/** Route un élément audio vers la sortie choisie ("" = sortie par défaut du système). */
export function applySink(el: HTMLAudioElement): void {
  const p = loadMediaPrefs();
  void (el as HTMLAudioElement & { setSinkId?: (id: string) => Promise<void> })
    .setSinkId?.(p.outputId ?? "")
    .catch(() => {}); // périphérique débranché → on reste sur la sortie par défaut
}
