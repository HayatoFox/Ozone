import { useCallback, useEffect, useRef, useState } from "react";
import * as Popover from "@radix-ui/react-popover";
import { Check, ChevronDown, Mic, RefreshCw, Video, VideoOff, Volume2 } from "lucide-react";
import { useStore } from "../store";
import {
  audioConstraints,
  comboLabel,
  videoConstraints,
  type KeyCombo,
  type MediaPrefs,
} from "../lib/mediaPrefs";
import { OVERLAY_ANIM } from "../lib/anim";

// Page « Voix et vidéo » : choix des périphériques (micro / sortie / caméra), test du micro
// (vumètre), aperçu caméra et traitement du son. Les préférences sont consommées par
// lib/voice.ts : sortie appliquée EN DIRECT, micro/caméra à la prochaine acquisition.
export function VoiceVideoSection() {
  const prefs = useStore((s) => s.mediaPrefs);
  const setPrefs = useStore((s) => s.setMediaPrefs);
  const inVoice = useStore((s) => !!s.myVoice);

  const [devices, setDevices] = useState<MediaDeviceInfo[]>([]);
  const [authNeeded, setAuthNeeded] = useState(false);

  const refresh = useCallback(async () => {
    try {
      const list = await navigator.mediaDevices.enumerateDevices();
      setDevices(list);
      // Tant que l'accès n'a pas été accordé, les libellés sont vides → proposer l'autorisation.
      setAuthNeeded(list.some((d) => d.kind === "audioinput") && list.every((d) => !d.label));
    } catch {
      setDevices([]);
    }
  }, []);

  useEffect(() => {
    void refresh();
    navigator.mediaDevices.addEventListener("devicechange", refresh);
    return () => navigator.mediaDevices.removeEventListener("devicechange", refresh);
  }, [refresh]);

  // Demande un accès éphémère pour révéler les libellés des périphériques.
  async function authorize() {
    try {
      const s = await navigator.mediaDevices.getUserMedia({ audio: true, video: true }).catch(
        // Pas de caméra ? On retente en audio seul.
        () => navigator.mediaDevices.getUserMedia({ audio: true }),
      );
      s.getTracks().forEach((t) => t.stop());
    } catch {
      /* refusé — on laisse les libellés génériques */
    }
    await refresh();
  }

  const mics = devices.filter((d) => d.kind === "audioinput");
  const outs = devices.filter((d) => d.kind === "audiooutput");
  const cams = devices.filter((d) => d.kind === "videoinput");

  const set = (patch: Partial<MediaPrefs>) => setPrefs({ ...prefs, ...patch });

  return (
    <>
      <h2 className="mb-5 text-xl font-bold text-header">Voix et vidéo</h2>

      {inVoice && (
        <p className="mb-5 rounded-lg bg-selected px-4 py-2.5 text-sm text-normal ring-1 ring-accent/40">
          Tu es en vocal : la <span className="font-semibold">sortie audio</span> change
          immédiatement ; le <span className="font-semibold">micro</span> et la{" "}
          <span className="font-semibold">caméra</span> s'appliqueront à la prochaine connexion.
        </p>
      )}

      {authNeeded && (
        <div className="mb-5 flex items-center justify-between gap-4 rounded-lg bg-sidebar px-4 py-3 ring-1 ring-line">
          <p className="text-sm text-muted">
            Autorise l'accès au micro/à la caméra pour afficher le nom de tes périphériques.
          </p>
          <button
            onClick={() => void authorize()}
            className="shrink-0 rounded-lg btn-accent px-4 py-1.5 text-sm font-medium text-white"
          >
            Autoriser
          </button>
        </div>
      )}

      {/* ── Périphériques ── */}
      <div className="grid grid-cols-1 gap-5 lg:grid-cols-2">
        <DevicePick
          icon={<Mic size={15} />}
          label="Périphérique d'entrée"
          devices={mics}
          value={prefs.micId}
          fallback="Microphone"
          onChange={(id) => set({ micId: id })}
        />
        <DevicePick
          icon={<Volume2 size={15} />}
          label="Périphérique de sortie"
          devices={outs}
          value={prefs.outputId}
          fallback="Haut-parleurs"
          onChange={(id) => set({ outputId: id })}
        />
      </div>

      <MicTest micId={prefs.micId} />

      <div className="my-7 h-px bg-line" />

      {/* ── Caméra ── */}
      <div className="mb-4 max-w-[420px]">
        <DevicePick
          icon={<Video size={15} />}
          label="Caméra"
          devices={cams}
          value={prefs.camId}
          fallback="Caméra"
          onChange={(id) => set({ camId: id })}
        />
      </div>
      <CameraPreview camId={prefs.camId} />

      <div className="my-7 h-px bg-line" />

      {/* ── Mode d'entrée ── */}
      <h3 className="mb-1 text-xs font-bold uppercase tracking-wide text-subtext">
        Mode d'entrée
      </h3>
      <p className="mb-3 text-sm text-muted">
        En mode appuyer-pour-parler, le micro ne transmet que pendant que la touche est maintenue
        (fenêtre de l'app au premier plan).
      </p>
      <div className="mb-3 flex flex-col gap-2">
        <ModeRow
          label="Détection de la voix"
          desc="Le micro ne transmet que quand tu parles (selon la sensibilité réglée ci-dessous)."
          on={prefs.inputMode !== "ptt"}
          onPick={() => set({ inputMode: "voice" })}
        />
        <ModeRow
          label="Appuyer-pour-parler"
          desc="Le micro ne s'ouvre que pendant l'appui sur la touche choisie."
          on={prefs.inputMode === "ptt"}
          onPick={() => set({ inputMode: "ptt" })}
        />
      </div>
      {prefs.inputMode === "ptt" && (
        <div className="mb-3 flex items-center justify-between gap-4 rounded-lg bg-sidebar px-4 py-3">
          <span>
            <span className="block font-medium text-header">Touche d'activation</span>
            <span className="block text-sm text-muted">
              Clique puis appuie sur la touche à utiliser.
            </span>
          </span>
          <PttKeyPicker value={prefs.pttKey} onPick={(code) => set({ pttKey: code })} />
        </div>
      )}

      {/* Sensibilité de détection : auto (calibrage) ou seuil manuel, en mode « détection ». */}
      {prefs.inputMode !== "ptt" && <Sensitivity prefs={prefs} set={set} />}

      <div className="my-7 h-px bg-line" />

      {/* ── Raccourcis clavier ── */}
      <h3 className="mb-1 text-xs font-bold uppercase tracking-wide text-subtext">Raccourcis</h3>
      <p className="mb-3 text-sm text-muted">
        Actifs quand la fenêtre de l'app est au premier plan (hors saisie de texte).
      </p>
      <div className="mb-3 flex flex-col gap-2">
        <div className="flex items-center justify-between gap-4 rounded-lg bg-sidebar px-4 py-3">
          <span>
            <span className="block font-medium text-header">Couper / réactiver le micro</span>
            <span className="block text-sm text-muted">Bascule ta sourdine micro.</span>
          </span>
          <ComboKeyPicker value={prefs.muteKey} onPick={(c) => set({ muteKey: c })} />
        </div>
        <div className="flex items-center justify-between gap-4 rounded-lg bg-sidebar px-4 py-3">
          <span>
            <span className="block font-medium text-header">Couper / réactiver le son</span>
            <span className="block text-sm text-muted">
              Bascule ta sourdine casque (coupe aussi le micro).
            </span>
          </span>
          <ComboKeyPicker value={prefs.deafenKey} onPick={(c) => set({ deafenKey: c })} />
        </div>
      </div>

      <div className="my-7 h-px bg-line" />

      {/* ── Traitement du son ── */}
      <h3 className="mb-1 text-xs font-bold uppercase tracking-wide text-subtext">
        Traitement de la voix
      </h3>
      <p className="mb-3 text-sm text-muted">
        Appliqué au micro à la prochaine connexion au vocal.
      </p>
      <div className="flex flex-col gap-2">
        <ToggleRow
          label="Annulation d'écho"
          desc="Évite que ton micro capte le son de tes haut-parleurs."
          on={prefs.echoCancellation}
          onToggle={() => set({ echoCancellation: !prefs.echoCancellation })}
        />
        <ToggleRow
          label="Réduction de bruit"
          desc="Atténue les bruits de fond constants (ventilateur, clavier…)."
          on={prefs.noiseSuppression}
          onToggle={() => set({ noiseSuppression: !prefs.noiseSuppression })}
        />
        <ToggleRow
          label="Gain automatique"
          desc="Normalise automatiquement le volume de ta voix."
          on={prefs.autoGainControl}
          onToggle={() => set({ autoGainControl: !prefs.autoGainControl })}
        />
      </div>
    </>
  );
}

// Carte « radio » d'un mode d'entrée vocale.
function ModeRow({
  label,
  desc,
  on,
  onPick,
}: {
  label: string;
  desc: string;
  on: boolean;
  onPick: () => void;
}) {
  return (
    <button
      onClick={onPick}
      className={`flex w-full items-start gap-3 rounded-lg px-4 py-3 text-left ring-1 transition-colors ${
        on ? "bg-selected ring-accent/50" : "bg-sidebar ring-transparent hover:bg-hover"
      }`}
    >
      <span
        className={`mt-1 flex h-4 w-4 shrink-0 items-center justify-center rounded-full border-2 ${
          on ? "border-[var(--accent)]" : "border-muted"
        }`}
      >
        {on && <span className="h-2 w-2 rounded-full bg-[var(--accent)]" />}
      </span>
      <span>
        <span className="block font-medium text-header">{label}</span>
        <span className="block text-sm text-muted">{desc}</span>
      </span>
    </button>
  );
}

// ───────────────────────────── Sensibilité (détection de voix) ─────────────────────────────

// Toggle auto/manuel + curseur de seuil + vu-mètre live montrant le niveau du micro et, en
// manuel, le seuil au-dessus duquel ta voix est transmise. Le boost ×3 aligne l'échelle visuelle
// sur celle du « Test du micro » au-dessus, et le seuil affiché suit la même échelle.
function Sensitivity({
  prefs,
  set,
}: {
  prefs: MediaPrefs;
  set: (patch: Partial<MediaPrefs>) => void;
}) {
  const [level, setLevel] = useState(0); // niveau RMS courant (0..1, boosté)
  const cleanup = useRef<(() => void) | null>(null);

  // Écoute le micro en continu tant que la section est visible (échantillonnage léger).
  useEffect(() => {
    let stopped = false;
    (async () => {
      try {
        const stream = await navigator.mediaDevices.getUserMedia({ audio: audioConstraints() });
        if (stopped) {
          stream.getTracks().forEach((t) => t.stop());
          return;
        }
        const ctx = new AudioContext();
        const analyser = ctx.createAnalyser();
        analyser.fftSize = 512;
        ctx.createMediaStreamSource(stream).connect(analyser);
        const buf = new Uint8Array(analyser.fftSize);
        const timer = setInterval(() => {
          analyser.getByteTimeDomainData(buf);
          let sum = 0;
          for (let i = 0; i < buf.length; i++) {
            const v = (buf[i] - 128) / 128;
            sum += v * v;
          }
          setLevel(Math.min(1, Math.sqrt(sum / buf.length) * 3));
        }, 80);
        cleanup.current = () => {
          clearInterval(timer);
          stream.getTracks().forEach((t) => t.stop());
          void ctx.close().catch(() => {});
        };
      } catch {
        /* micro indisponible : pas de vu-mètre */
      }
    })();
    return () => {
      stopped = true;
      cleanup.current?.();
      cleanup.current = null;
    };
  }, []);

  // Position du marqueur de seuil sur la même échelle visuelle que le vu-mètre (×3, borné).
  const markerPct = Math.min(100, prefs.vadThreshold * 3 * 100);

  return (
    <div className="mb-3 rounded-lg bg-sidebar px-4 py-3">
      <button
        onClick={() => set({ vadAuto: !prefs.vadAuto })}
        className="flex w-full items-start justify-between gap-4 text-left"
      >
        <span>
          <span className="block font-medium text-header">Sensibilité automatique</span>
          <span className="block text-sm text-muted">
            Ozone détecte ta voix par rapport au bruit de fond. Désactive pour régler le seuil
            toi-même.
          </span>
        </span>
        <span
          role="switch"
          aria-checked={prefs.vadAuto}
          className={`mt-1 flex h-6 w-11 shrink-0 items-center rounded-full px-0.5 transition-colors duration-200 ${
            prefs.vadAuto ? "bg-online" : "bg-white/15"
          }`}
        >
          <span
            className={`h-5 w-5 rounded-full bg-white shadow-sm transition-transform duration-200 ease-[cubic-bezier(0.34,1.56,0.64,1)] ${
              prefs.vadAuto ? "translate-x-5" : "translate-x-0.5"
            }`}
          />
        </span>
      </button>

      {/* Vu-mètre live : barre aurora = niveau ; trait vertical = seuil (mode manuel). */}
      <div className="relative mt-3 h-2.5 w-full overflow-hidden rounded-full bg-deepest ring-1 ring-line">
        <div
          className="h-full rounded-full bg-aurora transition-[width] duration-75 ease-out"
          style={{ width: `${Math.round(level * 100)}%` }}
        />
        {!prefs.vadAuto && (
          <span
            className="absolute top-1/2 h-4 w-0.5 -translate-y-1/2 rounded-full bg-white shadow"
            style={{ left: `${markerPct}%` }}
          />
        )}
      </div>

      {!prefs.vadAuto && (
        <div className="mt-3">
          <input
            type="range"
            min={1}
            max={30}
            value={Math.round(prefs.vadThreshold * 100)}
            onChange={(e) => set({ vadThreshold: Number(e.target.value) / 100 })}
            className="w-full accent-[var(--accent)]"
          />
          <p className="mt-1 text-xs text-muted">
            Glisse vers la droite si du bruit de fond passe ; vers la gauche si ta voix est coupée.
          </p>
        </div>
      )}
    </div>
  );
}

// ───────────────────────────── Capture de raccourci (combo) ─────────────────────────────

// Bouton de capture d'un raccourci complet (Ctrl/Maj/Alt + touche). Ignore les frappes de
// modificateurs seuls (on attend une vraie touche) et Échap (annule).
function ComboKeyPicker({ value, onPick }: { value: KeyCombo; onPick: (c: KeyCombo) => void }) {
  const [capturing, setCapturing] = useState(false);
  useEffect(() => {
    if (!capturing) return;
    const onKey = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      if (e.code === "Escape") {
        setCapturing(false);
        return;
      }
      // Attendre une touche « réelle » (pas un modificateur isolé).
      if (/^(Control|Shift|Alt|Meta)(Left|Right)$/.test(e.code)) return;
      onPick({ code: e.code, ctrl: e.ctrlKey, shift: e.shiftKey, alt: e.altKey });
      setCapturing(false);
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [capturing, onPick]);
  return (
    <button
      onClick={() => setCapturing(true)}
      className={`min-w-[150px] rounded-lg px-4 py-1.5 text-sm font-medium ring-1 ${
        capturing
          ? "bg-selected text-header ring-accent animate-pulse"
          : "bg-deepest text-normal ring-line hover:bg-black/40"
      }`}
    >
      {capturing ? "Appuie sur un raccourci…" : comboLabel(value)}
    </button>
  );
}

// Libellé lisible d'un KeyboardEvent.code (« KeyV » → « V », « Backquote » → « ² / ` »…).
function keyLabel(code: string): string {
  if (code.startsWith("Key")) return code.slice(3);
  if (code.startsWith("Digit")) return code.slice(5);
  const map: Record<string, string> = {
    Backquote: "² / `",
    Space: "Espace",
    ShiftLeft: "Maj gauche",
    ShiftRight: "Maj droite",
    ControlLeft: "Ctrl gauche",
    ControlRight: "Ctrl droit",
    AltLeft: "Alt",
    AltRight: "Alt Gr",
    CapsLock: "Verr. Maj",
  };
  return map[code] ?? code;
}

// Bouton de capture de la touche PTT : clic → « Appuie sur une touche… » → capture du code.
function PttKeyPicker({ value, onPick }: { value: string; onPick: (code: string) => void }) {
  const [capturing, setCapturing] = useState(false);
  useEffect(() => {
    if (!capturing) return;
    const onKey = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      if (e.code !== "Escape") onPick(e.code);
      setCapturing(false);
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [capturing, onPick]);
  return (
    <button
      onClick={() => setCapturing(true)}
      className={`min-w-[120px] rounded-lg px-4 py-1.5 text-sm font-medium ring-1 ${
        capturing
          ? "bg-selected text-header ring-accent animate-pulse"
          : "bg-deepest text-normal ring-line hover:bg-black/40"
      }`}
    >
      {capturing ? "Appuie sur une touche…" : keyLabel(value)}
    </button>
  );
}

// ───────────────────────────── Sélecteur de périphérique ─────────────────────────────

function DevicePick({
  icon,
  label,
  devices,
  value,
  fallback,
  onChange,
}: {
  icon: React.ReactNode;
  label: string;
  devices: MediaDeviceInfo[];
  value: string | null;
  fallback: string; // libellé générique si le navigateur masque les noms
  onChange: (id: string | null) => void;
}) {
  const name = (d: MediaDeviceInfo, i: number) => d.label || `${fallback} ${i + 1}`;
  const current = devices.find((d) => d.deviceId === value);
  const currentLabel = value === null ? "Par défaut" : current ? name(current, devices.indexOf(current)) : "Périphérique débranché";

  return (
    <div>
      <div className="mb-1.5 flex items-center gap-1.5 text-xs font-bold uppercase tracking-wide text-subtext">
        {icon} {label}
      </div>
      <Popover.Root>
        <Popover.Trigger className="flex w-full items-center justify-between rounded-lg bg-deepest px-3 py-2.5 text-left text-sm text-normal outline-none ring-1 ring-transparent hover:bg-white/5 data-[state=open]:ring-accent">
          <span className="truncate">{currentLabel}</span>
          <ChevronDown size={16} className="shrink-0 text-muted" />
        </Popover.Trigger>
        <Popover.Portal>
          <Popover.Content
            align="start"
            sideOffset={4}
            className={`z-[70] max-h-[280px] w-[var(--radix-popover-trigger-width)] overflow-y-auto rounded-xl bg-floating p-1.5 shadow-pop ring-1 ring-line scroll-thin ${OVERLAY_ANIM}`}
          >
            <Opt active={value === null} onClick={() => onChange(null)}>
              Par défaut
            </Opt>
            {devices.map((d, i) => (
              <Opt key={d.deviceId || i} active={value === d.deviceId} onClick={() => onChange(d.deviceId)}>
                {name(d, i)}
              </Opt>
            ))}
            {devices.length === 0 && (
              <p className="px-2 py-2 text-xs text-muted">Aucun périphérique détecté.</p>
            )}
          </Popover.Content>
        </Popover.Portal>
      </Popover.Root>
    </div>
  );
}

function Opt({
  children,
  active,
  onClick,
}: {
  children: React.ReactNode;
  active: boolean;
  onClick: () => void;
}) {
  return (
    <Popover.Close
      onClick={onClick}
      className="flex w-full items-center justify-between gap-2 rounded-lg px-2 py-1.5 text-left text-sm text-normal outline-none hover:bg-hover"
    >
      <span className="truncate">{children}</span>
      {active && <Check size={15} className="shrink-0 text-accent" />}
    </Popover.Close>
  );
}

// ───────────────────────────── Test du micro (vumètre) ─────────────────────────────

function MicTest({ micId }: { micId: string | null }) {
  const [testing, setTesting] = useState(false);
  const [level, setLevel] = useState(0); // 0..1
  const cleanup = useRef<(() => void) | null>(null);

  const stop = useCallback(() => {
    cleanup.current?.();
    cleanup.current = null;
    setTesting(false);
    setLevel(0);
  }, []);

  const start = useCallback(async () => {
    cleanup.current?.();
    try {
      const stream = await navigator.mediaDevices.getUserMedia({ audio: audioConstraints() });
      const ctx = new AudioContext();
      const analyser = ctx.createAnalyser();
      analyser.fftSize = 512;
      ctx.createMediaStreamSource(stream).connect(analyser);
      const buf = new Uint8Array(analyser.fftSize);
      const timer = setInterval(() => {
        analyser.getByteTimeDomainData(buf);
        let sum = 0;
        for (let i = 0; i < buf.length; i++) {
          const v = (buf[i] - 128) / 128;
          sum += v * v;
        }
        // RMS → niveau perceptif (boost ×3, borné) pour un vumètre réactif.
        setLevel(Math.min(1, Math.sqrt(sum / buf.length) * 3));
      }, 80);
      cleanup.current = () => {
        clearInterval(timer);
        stream.getTracks().forEach((t) => t.stop());
        void ctx.close().catch(() => {});
      };
      setTesting(true);
    } catch {
      setTesting(false);
    }
  }, []);

  // Micro changé pendant le test → on redémarre sur le nouveau périphérique.
  useEffect(() => {
    if (testing) void start();
  }, [micId]); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => () => cleanup.current?.(), []);

  return (
    <div className="mt-5">
      <div className="mb-1.5 text-xs font-bold uppercase tracking-wide text-subtext">
        Test du micro
      </div>
      <div className="flex items-center gap-3">
        <button
          onClick={() => (testing ? stop() : void start())}
          className={`shrink-0 rounded-lg px-4 py-2 text-sm font-medium text-white ${
            testing ? "bg-dnd hover:opacity-90" : "btn-accent"
          }`}
        >
          {testing ? "Arrêter" : "Tester"}
        </button>
        {/* Vumètre : barre aurora qui suit le niveau RMS. */}
        <div className="h-2.5 flex-1 overflow-hidden rounded-full bg-deepest ring-1 ring-line">
          <div
            className="h-full rounded-full bg-aurora transition-[width] duration-100 ease-out"
            style={{ width: `${Math.round(level * 100)}%` }}
          />
        </div>
      </div>
      <p className="mt-1.5 text-xs text-muted">
        Parle dans ton micro : la barre doit s'animer. Le test utilise le périphérique et le
        traitement sélectionnés.
      </p>
    </div>
  );
}

// ───────────────────────────── Aperçu caméra ─────────────────────────────

function CameraPreview({ camId }: { camId: string | null }) {
  const [on, setOn] = useState(false);
  const [pending, setPending] = useState(false); // demande de permission en cours
  const [error, setError] = useState(false);
  const videoRef = useRef<HTMLVideoElement>(null);
  const cleanup = useRef<(() => void) | null>(null);

  const stop = useCallback(() => {
    cleanup.current?.();
    cleanup.current = null;
    setOn(false);
  }, []);

  const start = useCallback(async () => {
    cleanup.current?.();
    setError(false);
    setPending(true);
    try {
      const stream = await navigator.mediaDevices.getUserMedia({ video: videoConstraints() });
      if (videoRef.current) videoRef.current.srcObject = stream;
      cleanup.current = () => {
        stream.getTracks().forEach((t) => t.stop());
        if (videoRef.current) videoRef.current.srcObject = null;
      };
      setOn(true);
    } catch {
      setError(true);
      setOn(false);
    } finally {
      setPending(false);
    }
  }, []);

  // Caméra changée pendant l'aperçu → redémarre sur le nouveau périphérique.
  useEffect(() => {
    if (on) void start();
  }, [camId]); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => () => cleanup.current?.(), []);

  return (
    <div className="max-w-[420px]">
      <div className="relative flex aspect-video items-center justify-center overflow-hidden rounded-xl bg-deepest ring-1 ring-line surface-card">
        <video
          ref={videoRef}
          autoPlay
          playsInline
          muted
          className={`h-full w-full object-cover ${on ? "" : "hidden"}`}
          style={{ transform: "scaleX(-1)" }}
        />
        {!on && (
          <div className="flex flex-col items-center gap-2 py-10 text-center">
            <VideoOff size={28} className="text-muted" />
            <p className="text-sm text-muted">
              {pending
                ? "Autorise l'accès à la caméra dans le navigateur…"
                : error
                  ? "Caméra inaccessible (refusée ou déjà utilisée)."
                  : "Aperçu désactivé."}
            </p>
          </div>
        )}
        <button
          onClick={() => (on ? stop() : void start())}
          disabled={pending}
          className={`absolute bottom-3 left-1/2 -translate-x-1/2 rounded-full px-4 py-1.5 text-sm font-medium text-white shadow-md disabled:opacity-60 ${
            on ? "bg-dnd hover:opacity-90" : "btn-accent"
          }`}
        >
          {pending ? "Autorisation…" : on ? "Arrêter l'aperçu" : "Tester la caméra"}
        </button>
      </div>
      <p className="mt-1.5 flex items-center gap-1.5 text-xs text-muted">
        <RefreshCw size={12} /> L'aperçu bascule automatiquement si tu changes de caméra.
      </p>
    </div>
  );
}

// ───────────────────────────── Ligne interrupteur ─────────────────────────────

function ToggleRow({
  label,
  desc,
  on,
  onToggle,
}: {
  label: string;
  desc: string;
  on: boolean;
  onToggle: () => void;
}) {
  return (
    <button
      onClick={onToggle}
      className="flex items-start justify-between gap-4 rounded-lg bg-sidebar px-4 py-3 text-left transition-colors hover:bg-hover"
    >
      <span>
        <span className="block font-medium text-header">{label}</span>
        <span className="block text-sm text-muted">{desc}</span>
      </span>
      <span
        role="switch"
        aria-checked={on}
        className={`mt-1 flex h-6 w-11 shrink-0 items-center rounded-full px-0.5 transition-colors duration-200 ${
          on ? "bg-online" : "bg-white/15"
        }`}
      >
        <span className={`h-5 w-5 rounded-full bg-white shadow-sm transition-transform duration-200 ease-[cubic-bezier(0.34,1.56,0.64,1)] ${on ? "translate-x-5" : "translate-x-0.5"}`} />
      </span>
    </button>
  );
}
