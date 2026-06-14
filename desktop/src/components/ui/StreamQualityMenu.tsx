import * as Popover from "@radix-ui/react-popover";
import type { ReactNode } from "react";
import { Check, Gamepad2, MonitorPlay, MonitorUp, ScreenShare, Settings2, X } from "lucide-react";
import { useStore } from "../../store";
import { OVERLAY_ANIM } from "../../lib/anim";
import type { StreamFps, StreamHeight, StreamPreset } from "../../lib/mediaPrefs";

// Libellé d'une hauteur cible : 0 ⇒ « Source » (résolution native), sinon « <h>p ».
export function heightLabel(h: StreamHeight): string {
  return h === 0 ? "Source" : `${h}p`;
}

const RESOLUTIONS: StreamHeight[] = [720, 1080, 1440, 0];
const FRAMERATES: StreamFps[] = [15, 30, 60];

// Préréglages façon Discord (« Mode de streaming »). Le préréglage actif est mémorisé
// explicitement (mediaPrefs.streamPreset) → « Personnalisés » reste sélectionnable même quand
// height/fps coïncident avec un préréglage.
type Preset = {
  id: StreamPreset;
  label: string;
  hint: string;
  icon: ReactNode;
  height?: StreamHeight;
  fps?: StreamFps;
};
const PRESETS: Preset[] = [
  { id: "gaming", label: "Gaming", hint: "Vidéo plus fluide", icon: <Gamepad2 size={16} />, height: 1440, fps: 60 },
  { id: "screen", label: "Partage d'écran", hint: "Texte plus clair", icon: <MonitorPlay size={16} />, height: 1080, fps: 15 },
  { id: "custom", label: "Personnalisés", hint: "Réglages manuels", icon: <Settings2 size={16} /> },
];

// Modale de partage d'écran (façon Discord) : choix du mode/qualité PUIS lancement. Le déclencheur
// est fourni par l'appelant via `children`. Tant qu'aucun partage n'est en cours, un bouton
// « Partager l'écran » lance la capture (le sélecteur de source natif s'ouvre alors) ; pendant un
// partage, on peut changer la source ou arrêter, et tout ajustement qualité s'applique à chaud.
export function StreamQualityMenu({ children }: { children: ReactNode }) {
  const mediaPrefs = useStore((s) => s.mediaPrefs);
  const setMediaPrefs = useStore((s) => s.setMediaPrefs);
  const localScreen = useStore((s) => s.localScreen);
  const restream = useStore((s) => s.restreamWithCurrentQuality);
  const toggleScreen = useStore((s) => s.toggleScreenShare);

  // Applique un changement de préférence puis, si un partage est en cours, l'applique à chaud.
  function apply(patch: Partial<typeof mediaPrefs>) {
    setMediaPrefs({ ...mediaPrefs, ...patch });
    if (localScreen) void restream();
  }

  // Sélectionne un préréglage : pose le mode ET ses valeurs (gaming/screen), ou bascule en custom.
  function pickPreset(p: Preset) {
    if (p.id === "custom") {
      apply({ streamPreset: "custom" });
    } else {
      apply({ streamPreset: p.id, streamHeight: p.height!, streamFps: p.fps! });
    }
  }

  const activePreset = mediaPrefs.streamPreset;

  return (
    <Popover.Root>
      <Popover.Trigger asChild>{children}</Popover.Trigger>
      <Popover.Portal>
        <Popover.Content
          side="top"
          align="center"
          sideOffset={10}
          className={`z-[70] w-[300px] rounded-xl bg-floating p-1.5 shadow-pop ring-1 ring-line ${OVERLAY_ANIM}`}
        >
          <div className="px-2 pb-1 pt-1 text-[11px] font-bold uppercase tracking-wide text-subtext">
            Mode de streaming
          </div>

          {/* Préréglages — sélection radio (mémorisée explicitement). */}
          {PRESETS.map((p) => (
            <button
              key={p.id}
              type="button"
              onClick={() => pickPreset(p)}
              className={`pressable flex w-full items-center gap-2.5 rounded px-2 py-1.5 text-left transition-colors ${
                activePreset === p.id ? "bg-white/10 text-header" : "text-normal hover:bg-white/5"
              }`}
            >
              <span className="shrink-0 text-muted">{p.icon}</span>
              <span className="flex-1 truncate">
                <span className="block text-sm font-medium">{p.label}</span>
                <span className="block text-xs text-muted">{p.hint}</span>
              </span>
              <Radio on={activePreset === p.id} />
            </button>
          ))}

          {/* Réglages détaillés : visibles dès que « Personnalisés » est actif. */}
          {activePreset === "custom" && (
            <>
              <div className="my-1 h-px bg-white/10" />
              <div className="px-2 pb-1 pt-1 text-[11px] font-bold uppercase tracking-wide text-subtext">
                Résolution de l'écran
              </div>
              <div className="grid grid-cols-2 gap-1 px-1">
                {RESOLUTIONS.map((h) => (
                  <OptionPill
                    key={h}
                    on={mediaPrefs.streamHeight === h}
                    onClick={() => apply({ streamHeight: h })}
                  >
                    {heightLabel(h)}
                  </OptionPill>
                ))}
              </div>

              <div className="px-2 pb-1 pt-2 text-[11px] font-bold uppercase tracking-wide text-subtext">
                Images par seconde
              </div>
              <div className="grid grid-cols-3 gap-1 px-1">
                {FRAMERATES.map((f) => (
                  <OptionPill
                    key={f}
                    on={mediaPrefs.streamFps === f}
                    onClick={() => apply({ streamFps: f })}
                  >
                    {`${f}fps`}
                  </OptionPill>
                ))}
              </div>
            </>
          )}

          <div className="my-1 h-px bg-white/10" />

          {/* Couper l'audio du stream : coché ⇒ audio non partagé (streamAudio = false). */}
          <button
            type="button"
            onClick={() => apply({ streamAudio: !mediaPrefs.streamAudio })}
            className="pressable flex w-full items-center gap-2.5 rounded px-2 py-1.5 text-left text-sm text-normal transition-colors hover:bg-white/5"
          >
            <span
              className={`flex h-4 w-4 shrink-0 items-center justify-center rounded border transition-colors ${
                !mediaPrefs.streamAudio ? "border-transparent bg-accent text-white" : "border-white/25"
              }`}
            >
              {!mediaPrefs.streamAudio && <Check size={12} />}
            </span>
            <span className="flex-1 truncate">Couper l'audio du stream</span>
          </button>

          {/* Pendant un partage : changer de source. */}
          {localScreen && (
            <button
              type="button"
              onClick={() => void restream(true)}
              className="pressable flex w-full items-center gap-2.5 rounded px-2 py-1.5 text-left text-sm text-normal transition-colors hover:bg-white/5"
            >
              <span className="shrink-0 text-muted">
                <MonitorUp size={16} />
              </span>
              <span className="flex-1 truncate">Changer de source</span>
            </button>
          )}

          <div className="my-1 h-px bg-white/10" />

          {/* Action principale : lancer le partage (sélecteur de source natif) ou l'arrêter.
              C'est ce bouton — et non un démarrage automatique — qui déclenche getDisplayMedia. */}
          {localScreen ? (
            <Popover.Close asChild>
              <button
                type="button"
                onClick={() => void toggleScreen()}
                className="pressable mt-0.5 flex w-full items-center justify-center gap-2 rounded-lg bg-dnd py-2 text-sm font-semibold text-white transition-opacity hover:opacity-90"
              >
                <X size={16} />
                Arrêter le partage
              </button>
            </Popover.Close>
          ) : (
            <Popover.Close asChild>
              <button
                type="button"
                onClick={() => void toggleScreen()}
                className="pressable mt-0.5 flex w-full items-center justify-center gap-2 rounded-lg btn-success py-2 text-sm font-semibold text-white"
              >
                <ScreenShare size={16} />
                Partager l'écran
              </button>
            </Popover.Close>
          )}

          <Popover.Arrow className="fill-floating" />
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
}

// Pastille radio (sélection de préréglage).
function Radio({ on }: { on: boolean }) {
  return (
    <span
      className={`flex h-4 w-4 shrink-0 items-center justify-center rounded-full border transition-colors ${
        on ? "border-accent" : "border-white/25"
      }`}
    >
      {on && <span className="h-2 w-2 rounded-full bg-accent" />}
    </span>
  );
}

// Bouton-option (résolution / FPS) en grille — sélection radio.
function OptionPill({
  children,
  on,
  onClick,
}: {
  children: ReactNode;
  on: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={`pressable rounded-md px-2 py-1.5 text-sm font-medium transition-colors ${
        on ? "bg-accent text-white" : "bg-white/5 text-normal hover:bg-white/10"
      }`}
    >
      {children}
    </button>
  );
}
