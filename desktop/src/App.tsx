import { useEffect } from "react";
import { useStore } from "./store";
import { CH_VOICE } from "./types";
import { AuthScreen } from "./components/AuthScreen";
import { ServerRail } from "./components/ServerRail";
import { ChannelSidebar } from "./components/ChannelSidebar";
import { ChatView } from "./components/ChatView";
import { MemberList } from "./components/MemberList";
import { DMProfilePanel } from "./components/DMProfilePanel";
import { FriendsView } from "./components/FriendsView";
import { Settings } from "./components/Settings";
import { TooltipProvider } from "./components/ui/Tooltip";
import { Spinner } from "./components/ui/Spinner";

export function App() {
  const ready = useStore((s) => s.ready);
  const authed = useStore((s) => s.authed);
  const instance = useStore((s) => s.instance);
  const boot = useStore((s) => s.boot);

  useEffect(() => {
    void boot();
  }, [boot]);

  // Application native : le menu contextuel du NAVIGATEUR ne doit jamais apparaître.
  // Les menus Radix (salons, messages, serveurs…) s'ouvrent au niveau de leur déclencheur
  // (phase de bouillonnement interne) avant ce gestionnaire document → ils fonctionnent normalement.
  useEffect(() => {
    const suppress = (e: MouseEvent) => e.preventDefault();
    document.addEventListener("contextmenu", suppress);
    return () => document.removeEventListener("contextmenu", suppress);
  }, []);

  if (!ready) {
    return (
      <div className="aurora-halo flex h-full w-full flex-col items-center justify-center gap-5 bg-deepest">
        <div className="flex h-16 w-16 items-center justify-center rounded-2xl bg-aurora text-xl font-bold text-white shadow-lg">
          Oz
        </div>
        <div className="h-1.5 w-40 overflow-hidden rounded-full bg-white/10">
          <div className="h-full w-1/3 animate-loadbar rounded-full bg-aurora" />
        </div>
      </div>
    );
  }

  if (!authed) return <AuthScreen instance={instance} />;

  return (
    <TooltipProvider delayDuration={300} skipDelayDuration={200}>
      <MainShell />
    </TooltipProvider>
  );
}

function MainShell() {
  const view = useStore((s) => s.view);
  const activeDM = useStore((s) => s.activeDM);
  const status = useStore((s) => s.gatewayStatus);
  const error = useStore((s) => s.error);
  const setError = useStore((s) => s.setError);
  const settingsOpen = useStore((s) => s.settingsOpen);
  const setSettingsOpen = useStore((s) => s.setSettingsOpen);
  const dmProfileOpen = useStore((s) => s.dmProfileOpen);
  const voiceTextOpen = useStore((s) => s.voiceTextOpen);
  const activeDmChannel = useStore((s) =>
    s.activeDM ? s.dms.find((d) => d.id === s.activeDM) : undefined,
  );
  // Salon vocal actif → la discussion textuelle intégrée occupe la colonne de droite
  // (on masque alors la liste des membres pour ne pas avoir deux panneaux à droite).
  const guildVoiceActive = useStore((s) => {
    if (s.view.kind !== "guild") return false;
    const sel = s.selectedChannelByGuild[s.view.guildId];
    return s.channelsByGuild[s.view.guildId]?.find((c) => c.id === sel)?.type === CH_VOICE;
  });

  // Accueil sans MP actif → vue Amis ; sinon vue de chat.
  const showFriends = view.kind === "home" && activeDM === null;

  return (
    <div className="flex h-full w-full overflow-hidden">
      <ServerRail />
      {/* Coque « carte flottante » : tout le contenu vit dans une carte arrondie posée sur le
          fond profond de la fenêtre (hiérarchie de profondeur, rien de brut). */}
      <div className="my-2 mr-2 flex min-w-0 flex-1 overflow-hidden rounded-2xl bg-chat shadow-xl ring-1 ring-cardline">
        <ChannelSidebar />
        {showFriends ? <FriendsView /> : <ChatView />}
        {view.kind === "guild" && !(guildVoiceActive && voiceTextOpen) && (
          <MemberList guildId={view.guildId} />
        )}
        {!showFriends && view.kind === "home" && dmProfileOpen && activeDmChannel && (
          <DMProfilePanel dm={activeDmChannel} />
        )}
      </div>

      {settingsOpen && <Settings onClose={() => setSettingsOpen(false)} />}

      {status === "disconnected" && (
        <div
          role="status"
          className="animate-in fade-in slide-in-from-bottom-2 duration-300 pointer-events-none fixed bottom-3 left-1/2 flex -translate-x-1/2 items-center gap-2 rounded-full bg-dnd/90 px-3.5 py-1.5 text-sm text-white shadow-lg"
        >
          <Spinner size={13} />
          Reconnexion à la Gateway…
        </div>
      )}
      {error && (
        <button
          onClick={() => setError(null)}
          role="alert"
          className="pressable animate-in fade-in slide-in-from-bottom-2 duration-300 fixed bottom-3 right-3 max-w-sm rounded-xl bg-dnd px-4 py-2.5 text-left text-sm text-white shadow-lg ring-1 ring-white/10"
        >
          {error} <span className="opacity-70">(cliquer pour fermer)</span>
        </button>
      )}
    </div>
  );
}
