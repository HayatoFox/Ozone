import { useEffect, useRef, useState } from "react";
import {
  Archive,
  AtSign,
  BarChart3,
  Check,
  EyeOff,
  Hash,
  Lock,
  MessagesSquare,
  PanelRight,
  Paperclip,
  PlusCircle,
  Smile,
  Sticker as StickerIcon,
  Timer,
  Upload,
  UserPlus,
  X,
} from "lucide-react";
import { api } from "../api";
import { canIn, roleColorHex, useStore } from "../store";
import { PERM } from "../lib/permissions";
import { displayName } from "../lib/format";
import { mediaUrl } from "../lib/instance";
import { MessageList } from "./MessageList";
import { InboxButton, PinsButton, SearchButton } from "./HeaderActions";
import { AddToGroupModal } from "./AddToGroupModal";
import { CreatePollModal } from "./CreatePollModal";
import { ChatSkeleton } from "./ui/Skeleton";
import { EmojiPicker } from "./ui/EmojiPicker";
import { StickerPicker } from "./ui/StickerPicker";
import { Avatar } from "./Avatar";
import { VoiceStage } from "./VoiceStage";
import { VoiceTextChat } from "./VoiceTextChat";
import { CH_TEXT, CH_VOICE, isThreadType, type Attachment, type Channel, type Message } from "../types";

// Taille maximale d'une pièce jointe (1 Go) — alignée sur le plafond serveur.
const MAX_ATTACHMENT_BYTES = 1024 * 1024 * 1024;

// Upload « stagé » : un fichier choisi s'upload en arrière-plan, annulable, et le message
// n'est envoyé qu'une fois tous les uploads terminés.
export interface StagedUpload {
  localId: string;
  name: string;
  size: number;
  type: string;
  status: "uploading" | "done" | "error";
  attachment?: Attachment;
  abort: () => void;
}

// Détermine le salon actuellement affiché (guilde ou MP).
function useActiveChannelId(): string | null {
  const view = useStore((s) => s.view);
  const activeDM = useStore((s) => s.activeDM);
  const selectedByGuild = useStore((s) => s.selectedChannelByGuild);
  if (view.kind === "guild") return selectedByGuild[view.guildId] ?? null;
  return activeDM;
}

export function ChatView() {
  const channelId = useActiveChannelId();
  const view = useStore((s) => s.view);
  const channel = useStore((s) => {
    if (view.kind !== "guild" || !channelId) return undefined;
    const inList = s.channelsByGuild[view.guildId]?.find((c) => c.id === channelId);
    if (inList) return inList;
    // Fils : chercher dans threadsByChannel.
    for (const threads of Object.values(s.threadsByChannel)) {
      const t = threads.find((c) => c.id === channelId);
      if (t) return t;
    }
    return undefined;
  });
  const dm = useStore((s) => (channelId ? s.dms.find((d) => d.id === channelId) : undefined));
  const me = useStore((s) => s.me);
  const setError = useStore((s) => s.setError);
  const messages = useStore((s) => (channelId ? s.messagesByChannel[channelId] : undefined));
  const dmProfileOpen = useStore((s) => s.dmProfileOpen);
  const toggleDmProfile = useStore((s) => s.toggleDmProfile);
  const voiceTextOpen = useStore((s) => s.voiceTextOpen);
  const [replyTarget, setReplyTarget] = useState<Message | null>(null);
  // Uploads « stagés » : chaque fichier choisi s'upload en arrière-plan (annulable) ; le message
  // n'est envoyé qu'une fois tous les uploads terminés.
  const [uploads, setUploads] = useState<StagedUpload[]>([]);
  const [dragging, setDragging] = useState(false);
  const [addToGroup, setAddToGroup] = useState(false);
  const dragDepth = useRef(0);

  // Salon courant suivi par ref : les uploads asynchrones comparent le salon CIBLE capturé à
  // l'appel avec le salon actif au moment de la résolution (anti-race au changement de salon).
  const currentChannelRef = useRef(channelId);
  useEffect(() => {
    currentChannelRef.current = channelId;
  });

  // Changement de salon : la réponse en cours et les pièces jointes appartiennent à
  // l'ancien salon → on les réinitialise (en annulant les uploads en cours).
  useEffect(() => {
    setReplyTarget(null);
    setUploads((u) => {
      u.forEach((x) => x.status === "uploading" && x.abort());
      return [];
    });
  }, [channelId]);

  if (!channelId) {
    return (
      <div className="aurora-halo flex flex-1 flex-col items-center justify-center gap-3 bg-chat text-center">
        <div className="flex h-16 w-16 items-center justify-center rounded-2xl bg-hover ring-1 ring-line surface-card">
          <MessagesSquare size={30} style={{ color: "var(--aurora-a)" }} />
        </div>
        <div>
          <p className="font-semibold text-header">Aucun salon sélectionné</p>
          <p className="text-sm text-muted">Choisis un salon à gauche pour commencer à discuter.</p>
        </div>
      </div>
    );
  }

  // Salon vocal → scène d'appel + discussion textuelle intégrée (panneau de droite, façon Discord).
  if (channel && channel.type === CH_VOICE && view.kind === "guild") {
    return (
      <div className="flex min-w-0 flex-1">
        <VoiceStage channel={channel} guildId={view.guildId} />
        {voiceTextOpen && <VoiceTextChat channel={channel} guildId={view.guildId} />}
      </div>
    );
  }

  const title =
    channel?.name ??
    (dm
      ? dm.name || dm.recipients.filter((u) => u.id !== me?.id).map(displayName).join(", ")
      : "Salon");

  function onFiles(files: FileList | File[] | null) {
    if (!channelId || !files || files.length === 0) return;
    const target = channelId; // salon cible figé pour toute la durée de l'upload
    for (const f of Array.from(files)) {
      if (f.size > MAX_ATTACHMENT_BYTES) {
        setError(`« ${f.name} » dépasse la limite de 1 Go.`);
        continue;
      }
      const localId = `${f.name}-${f.size}-${performance.now()}-${Math.random()}`;
      const controller = new AbortController();
      const entry: StagedUpload = {
        localId,
        name: f.name,
        size: f.size,
        type: f.type,
        status: "uploading",
        abort: () => controller.abort(),
      };
      setUploads((u) => [...u, entry]);
      // Upload en arrière-plan (annulable). On NE bloque PAS : l'utilisateur prépare son message
      // pendant ce temps ; l'envoi attendra la fin (cf. Composer).
      api
        .uploadAttachmentAbortable(target, f, controller.signal)
        .then((att) => {
          // Salon changé entre-temps → la PJ appartient à l'ancien salon : on l'abandonne.
          if (currentChannelRef.current !== target) {
            setUploads((u) => u.filter((x) => x.localId !== localId));
            return;
          }
          setUploads((u) =>
            u.map((x) => (x.localId === localId ? { ...x, status: "done", attachment: att } : x)),
          );
        })
        .catch((e) => {
          // Annulation volontaire → on retire l'entrée silencieusement.
          if (controller.signal.aborted) {
            setUploads((u) => u.filter((x) => x.localId !== localId));
            return;
          }
          setUploads((u) => u.map((x) => (x.localId === localId ? { ...x, status: "error" } : x)));
          if (currentChannelRef.current === target) {
            setError(e instanceof Error ? e.message : "Échec du téléversement.");
          }
        });
    }
  }

  function cancelUpload(localId: string) {
    setUploads((u) => {
      const entry = u.find((x) => x.localId === localId);
      entry?.abort();
      return u.filter((x) => x.localId !== localId);
    });
  }

  return (
    <div
      className="relative flex flex-1 flex-col bg-chat"
      onDragEnter={(e) => {
        e.preventDefault();
        dragDepth.current += 1;
        if (e.dataTransfer.types.includes("Files")) setDragging(true);
      }}
      onDragOver={(e) => e.preventDefault()}
      onDragLeave={() => {
        dragDepth.current -= 1;
        if (dragDepth.current <= 0) setDragging(false);
      }}
      onDrop={(e) => {
        e.preventDefault();
        dragDepth.current = 0;
        setDragging(false);
        void onFiles(e.dataTransfer.files);
      }}
    >
      <header className="flex h-12 shrink-0 items-center gap-2 border-b border-line px-4 shadow-sm">
        {channel ? (
          <Hash size={22} className="text-muted" />
        ) : (
          <AtSign size={22} className="text-muted" />
        )}
        <h2 className="font-semibold text-header">{title}</h2>
        {channel?.topic && (
          <>
            <span className="mx-1 h-5 w-px bg-white/10" />
            <span className="truncate text-sm text-muted">{channel.topic}</span>
          </>
        )}
        <div className="ml-auto flex items-center gap-4">
          {channel && isThreadType(channel.type) && view.kind === "guild" && (
            <ThreadControls channel={channel} guildId={view.guildId} />
          )}
          <InboxButton />
          <PinsButton channelId={channelId} />
          {dm && (
            <button
              onClick={() => setAddToGroup(true)}
              title="Ajouter au MP"
              className="pressable text-interactive-normal outline-none transition-colors hover:text-interactive-hover"
            >
              <UserPlus size={20} />
            </button>
          )}
          <SearchButton channelId={channelId} />
          {dm && (
            <button
              onClick={toggleDmProfile}
              title={dmProfileOpen ? "Masquer le profil d'utilisateur" : "Afficher le profil d'utilisateur"}
              className={`pressable outline-none transition-colors ${
                dmProfileOpen ? "text-interactive-active" : "text-interactive-normal hover:text-interactive-hover"
              }`}
            >
              <PanelRight size={20} />
            </button>
          )}
        </div>
      </header>

      {messages === undefined ? (
        <ChatSkeleton />
      ) : (
        <MessageList
          key={channelId}
          messages={messages}
          channelId={channelId}
          guildId={view.kind === "guild" ? view.guildId : undefined}
          onReply={setReplyTarget}
          channelName={title}
          dm={!channel}
        />
      )}

      <TypingIndicator channelId={channelId} />
      <Composer
        key={`composer-${channelId}`}
        channelId={channelId}
        title={title}
        guildId={view.kind === "guild" ? view.guildId : undefined}
        replyTarget={replyTarget}
        onClearReply={() => setReplyTarget(null)}
        uploads={uploads}
        setUploads={setUploads}
        onCancelUpload={cancelUpload}
        onFiles={onFiles}
      />

      {dragging && (
        <div className="pointer-events-none absolute inset-2 z-40 flex flex-col items-center justify-center rounded-xl border-2 border-dashed border-accent bg-accent/10">
          <Upload size={48} className="text-accent" />
          <p className="mt-3 text-lg font-semibold text-header">Dépose pour téléverser</p>
        </div>
      )}

      {addToGroup && dm && <AddToGroupModal dm={dm} onClose={() => setAddToGroup(false)} />}
    </div>
  );
}

function TypingIndicator({ channelId }: { channelId: string }) {
  const typing = useStore((s) => s.typing[channelId]);
  const [, force] = useState(0);

  useEffect(() => {
    const t = setInterval(() => force((n) => n + 1), 1000);
    return () => clearInterval(t);
  }, []);

  if (!typing) return <div className="h-6" />;
  const now = performance.now();
  const count = Object.values(typing).filter((exp) => exp > now).length;
  if (count === 0) return <div className="h-6" />;
  return (
    <div className="flex h-6 items-center gap-2 px-4 text-xs text-muted">
      <span className="flex items-center gap-0.5">
        {[0, 1, 2].map((i) => (
          <span
            key={i}
            className="h-1.5 w-1.5 animate-bounce rounded-full bg-muted"
            style={{ animationDelay: `${i * 150}ms`, animationDuration: "0.9s" }}
          />
        ))}
      </span>
      <span>
        {count} personne{count > 1 ? "s" : ""} en train d'écrire…
      </span>
    </div>
  );
}

export function Composer({
  channelId,
  title,
  guildId,
  replyTarget,
  onClearReply,
  uploads,
  setUploads,
  onCancelUpload,
  onFiles,
}: {
  channelId: string;
  title: string;
  guildId?: string;
  replyTarget: Message | null;
  onClearReply: () => void;
  uploads: StagedUpload[];
  setUploads: React.Dispatch<React.SetStateAction<StagedUpload[]>>;
  onCancelUpload: (localId: string) => void;
  onFiles: (files: FileList | File[] | null) => void | Promise<void>;
}) {
  const [text, setText] = useState("");
  const [pollOpen, setPollOpen] = useState(false);
  // Envoi demandé alors que des uploads tournent encore → on enverra dès qu'ils sont finis.
  const [sendRequested, setSendRequested] = useState(false);
  const uploading = uploads.some((u) => u.status === "uploading");
  const sendMessage = useStore((s) => s.sendMessage);
  const customEmojis = useStore((s) => (guildId ? s.emojisByGuild[guildId] : undefined));
  const stickers = useStore((s) => (guildId ? s.stickersByGuild[guildId] : undefined));
  const lastTyping = useRef(0);
  const fileInput = useRef<HTMLInputElement>(null);
  const spoilerInput = useRef<HTMLInputElement>(null);
  const taRef = useRef<HTMLTextAreaElement>(null);

  // Hauteur auto du composeur (jusqu'à ~8 lignes).
  useEffect(() => {
    const el = taRef.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${Math.min(el.scrollHeight, 200)}px`;
  }, [text]);

  // Autocomplétion d'emoji : si le texte se termine par `:partiel`, propose les emoji de la guilde.
  const emojiQuery = /(?:^|\s):(\w{2,32})$/.exec(text)?.[1]?.toLowerCase();
  const suggestions =
    emojiQuery && customEmojis
      ? customEmojis.filter((e) => e.name.toLowerCase().startsWith(emojiQuery)).slice(0, 8)
      : [];

  function insertEmoji(e: { name: string; id: string; animated: boolean }) {
    const token = `<${e.animated ? "a" : ""}:${e.name}:${e.id}> `;
    setText((t) => t.replace(/:(\w{2,32})$/, token));
  }

  // Autocomplétion des @mentions (membres + rôles) et #salons (guilde uniquement).
  const members = useStore((s) => (guildId ? s.membersByGuild[guildId] : undefined));
  const guildChannels = useStore((s) => (guildId ? s.channelsByGuild[guildId] : undefined));
  const guildRoles = useStore((s) => (guildId ? s.rolesByGuild[guildId] : undefined));
  const canMentionAll = useStore((s) => (guildId ? canIn(s, guildId, PERM.MENTION_EVERYONE) : false));
  const mentionQuery = guildId ? /(?:^|\s)@([\w.-]{0,32})$/.exec(text)?.[1]?.toLowerCase() : undefined;
  const channelQuery = guildId ? /(?:^|\s)#([\w-]{0,32})$/.exec(text)?.[1]?.toLowerCase() : undefined;
  const mentionSuggestions =
    mentionQuery !== undefined
      ? (members ?? [])
          .filter(
            (m) =>
              (m.nick || displayName(m.user)).toLowerCase().includes(mentionQuery) ||
              m.user.username.toLowerCase().includes(mentionQuery),
          )
          .slice(0, 8)
      : [];
  // Rôles proposés : mentionnables par tous, ou tous si l'auteur a MENTION_EVERYONE.
  const roleSuggestions =
    mentionQuery !== undefined
      ? (guildRoles ?? [])
          .filter((r) => r.id !== guildId && !r.managed && (r.mentionable || canMentionAll))
          .filter((r) => r.name.toLowerCase().includes(mentionQuery))
          .slice(0, 5)
      : [];
  const channelSuggestions =
    channelQuery !== undefined
      ? (guildChannels ?? [])
          .filter((c) => c.type === CH_TEXT && c.name.toLowerCase().includes(channelQuery))
          .slice(0, 8)
      : [];

  function insertMention(userId: string) {
    setText((t) => t.replace(/@([\w.-]{0,32})$/, `<@${userId}> `));
  }
  function insertRoleMention(roleId: string) {
    setText((t) => t.replace(/@([\w.-]{0,32})$/, `<@&${roleId}> `));
  }
  function insertChannelRef(channelRefId: string) {
    setText((t) => t.replace(/#([\w-]{0,32})$/, `<#${channelRefId}> `));
  }

  // ── Slowmode : compte à rebours local après chaque envoi (les modérateurs sont exemptés,
  // comme côté serveur — MANAGE_MESSAGES / MANAGE_CHANNELS). Le serveur reste l'autorité.
  const channel = useStore((s) =>
    guildId ? s.channelsByGuild[guildId]?.find((c) => c.id === channelId) : undefined,
  );
  const slowmode = channel?.rate_limit_per_user ?? 0;
  const slowmodeExempt = useStore(
    (s) =>
      !guildId ||
      canIn(s, guildId, PERM.MANAGE_MESSAGES) ||
      canIn(s, guildId, PERM.MANAGE_CHANNELS),
  );
  const [cooldownLeft, setCooldownLeft] = useState(0);
  useEffect(() => {
    if (cooldownLeft <= 0) return;
    const t = setInterval(
      () => setCooldownLeft((v) => Math.max(0, v - 1)),
      1000,
    );
    return () => clearInterval(t);
  }, [cooldownLeft > 0]); // eslint-disable-line react-hooks/exhaustive-deps
  useEffect(() => setCooldownLeft(0), [channelId]); // changement de salon → pas de report

  // Envoi RÉEL : appelé une fois tous les uploads terminés (ou immédiatement s'il n'y en a pas).
  async function doSend(content: string) {
    const done = uploads.filter((u) => u.status === "done" && u.attachment);
    const attachments = done.map((u) => u.attachment!.id);
    if (!content && attachments.length === 0) return;
    const replyTo = replyTarget?.id;
    setText("");
    setUploads([]);
    onClearReply();
    const ok = await sendMessage(channelId, content, { attachments, replyTo });
    if (!ok) {
      setText((cur) => cur || content); // échec : on restaure le brouillon
    } else if (slowmode > 0 && !slowmodeExempt) {
      setCooldownLeft(slowmode);
    }
  }

  function submit() {
    const content = text.trim();
    const hasUploads = uploads.length > 0;
    if (!content && !hasUploads) return;
    if (slowmode > 0 && !slowmodeExempt && cooldownLeft > 0) return;
    // Des uploads tournent encore → on diffère l'envoi (le message part dès qu'ils sont finis).
    if (uploads.some((u) => u.status === "uploading")) {
      setSendRequested(true);
      return;
    }
    void doSend(content);
  }

  // Envoi différé : quand tous les uploads sont terminés et qu'un envoi a été demandé, on part.
  useEffect(() => {
    if (!sendRequested) return;
    if (uploads.some((u) => u.status === "uploading")) return; // pas encore prêt
    setSendRequested(false);
    void doSend(text.trim());
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sendRequested, uploads]);

  function onChange(v: string) {
    setText(v);
    const now = performance.now();
    if (now - lastTyping.current > 4000) {
      lastTyping.current = now;
      void api.typing(channelId).catch(() => {});
    }
  }

  return (
    <div className="px-4 pb-6">
      {replyTarget && (
        <div className="flex animate-accordion items-center justify-between rounded-t-xl bg-userpanel px-4 py-1.5 text-sm text-muted">
          <span>
            Réponse à{" "}
            <span className="font-medium text-header">{displayName(replyTarget.author)}</span>
          </span>
          <button onClick={onClearReply} className="pressable rounded p-0.5 transition-colors hover:bg-hover hover:text-normal" title="Annuler">
            <X size={16} />
          </button>
        </div>
      )}

      {uploads.length > 0 && (
        <div className="flex animate-accordion flex-wrap gap-2 rounded-t-xl bg-userpanel px-4 py-2">
          {uploads.map((u) => (
            <div
              key={u.localId}
              className="group/att relative w-40 animate-pop-in overflow-hidden rounded bg-sidebar p-2 ring-1 ring-line"
            >
              <div className="flex items-center gap-2">
                <Paperclip size={14} className="shrink-0 text-muted" />
                <span className="min-w-0 flex-1 truncate text-xs text-normal" title={u.name}>
                  {u.name.startsWith("SPOILER_") ? u.name.slice(8) : u.name}
                </span>
              </div>
              {/* État de l'upload : barre animée pendant le transfert, coche/erreur ensuite. */}
              {u.status === "uploading" && (
                <div className="mt-1.5 h-1 overflow-hidden rounded-full bg-black/30">
                  <div className="h-full w-1/3 animate-indeterminate rounded-full bg-accent" />
                </div>
              )}
              {u.status === "done" && (
                <div className="mt-1 flex items-center gap-1 text-[11px] text-online">
                  <Check size={11} /> Prêt
                </div>
              )}
              {u.status === "error" && (
                <div className="mt-1 text-[11px] text-dnd">Échec</div>
              )}
              <button
                onClick={() => onCancelUpload(u.localId)}
                className="pressable absolute -right-1.5 -top-1.5 rounded-full bg-dnd p-0.5 text-white shadow-sm transition-transform hover:scale-110"
                title={u.status === "uploading" ? "Annuler l'upload" : "Retirer"}
              >
                <X size={12} />
              </button>
            </div>
          ))}
        </div>
      )}

      {suggestions.length > 0 && (
        <div className="mb-1 origin-bottom animate-pop-in overflow-hidden rounded-xl bg-floating shadow-pop ring-1 ring-cardline">
          <div className="px-3 py-1.5 text-xs font-semibold uppercase text-muted">Emoji</div>
          {suggestions.map((e) => (
            <button
              key={e.id}
              onClick={() => insertEmoji(e)}
              className="flex w-full items-center gap-2 px-3 py-1.5 text-left transition-colors hover:bg-hover"
            >
              <img src={mediaUrl(`/api/emojis/${e.id}`)} alt="" className="h-5 w-5 object-contain" />
              <span className="text-sm text-normal">:{e.name}:</span>
            </button>
          ))}
        </div>
      )}

      {(mentionSuggestions.length > 0 || roleSuggestions.length > 0) && (
        <div className="mb-1 origin-bottom animate-pop-in overflow-hidden rounded-xl bg-floating shadow-pop ring-1 ring-cardline">
          {roleSuggestions.length > 0 && (
            <>
              <div className="px-3 py-1.5 text-xs font-semibold uppercase text-muted">Rôles</div>
              {roleSuggestions.map((r) => (
                <button
                  key={r.id}
                  onClick={() => insertRoleMention(r.id)}
                  className="flex w-full items-center gap-2 px-3 py-1.5 text-left transition-colors hover:bg-hover"
                >
                  <span
                    className="h-2.5 w-2.5 shrink-0 rounded-full"
                    style={{ backgroundColor: r.color ? roleColorHex(r.color) : "#99aab5" }}
                  />
                  <span className="text-sm text-normal">@{r.name}</span>
                </button>
              ))}
            </>
          )}
          {mentionSuggestions.length > 0 && (
            <>
              <div className="px-3 py-1.5 text-xs font-semibold uppercase text-muted">Membres</div>
              {mentionSuggestions.map((m) => (
                <button
                  key={m.user.id}
                  onClick={() => insertMention(m.user.id)}
                  className="flex w-full items-center gap-2 px-3 py-1.5 text-left transition-colors hover:bg-hover"
                >
                  <Avatar name={m.nick || displayName(m.user)} id={m.user.id} size={22} avatarId={m.user.avatar_id} />
                  <span className="text-sm text-normal">{m.nick || displayName(m.user)}</span>
                  <span className="text-xs text-muted">{m.user.username}</span>
                </button>
              ))}
            </>
          )}
        </div>
      )}

      {channelSuggestions.length > 0 && (
        <div className="mb-1 origin-bottom animate-pop-in overflow-hidden rounded-xl bg-floating shadow-pop ring-1 ring-cardline">
          <div className="px-3 py-1.5 text-xs font-semibold uppercase text-muted">Salons</div>
          {channelSuggestions.map((c) => (
            <button
              key={c.id}
              onClick={() => insertChannelRef(c.id)}
              className="flex w-full items-center gap-2 px-3 py-1.5 text-left transition-colors hover:bg-hover"
            >
              <Hash size={16} className="text-muted" />
              <span className="text-sm text-normal">{c.name}</span>
            </button>
          ))}
        </div>
      )}

      <div
        className={`flex items-center gap-3 bg-field px-4 shadow-sm ring-1 ring-white/[0.04] transition-all duration-200 focus-within:ring-2 focus-within:ring-accent/50 ${
          replyTarget || uploads.length > 0 ? "rounded-b-xl" : "rounded-xl"
        }`}
      >
        <input
          ref={fileInput}
          type="file"
          multiple
          className="hidden"
          onChange={(e) => {
            void onFiles(e.target.files);
            if (fileInput.current) fileInput.current.value = "";
          }}
        />
        {/* Pièce jointe spoiler : préfixe « SPOILER_ » → floutée au rendu jusqu'au clic. */}
        <input
          ref={spoilerInput}
          type="file"
          multiple
          className="hidden"
          onChange={(e) => {
            const files = Array.from(e.target.files ?? []).map(
              (f) => new File([f], `SPOILER_${f.name}`, { type: f.type }),
            );
            if (files.length) void onFiles(files);
            if (spoilerInput.current) spoilerInput.current.value = "";
          }}
        />
        <button
          onClick={() => fileInput.current?.click()}
          title="Joindre un fichier (jusqu'à 1 Go)"
          className="pressable text-interactive-normal hover:text-interactive-hover"
        >
          <PlusCircle size={22} />
        </button>
        <button
          onClick={() => spoilerInput.current?.click()}
          title="Joindre en spoiler"
          className="pressable text-interactive-normal hover:text-interactive-hover"
        >
          <EyeOff size={20} />
        </button>
        <button
          onClick={() => setPollOpen(true)}
          title="Créer un sondage"
          className="pressable text-interactive-normal hover:text-interactive-hover"
        >
          <BarChart3 size={20} />
        </button>
        {guildId && (
          <StickerPicker
            stickers={stickers ?? []}
            trigger={
              <button
                title="Autocollant"
                className="pressable text-interactive-normal hover:text-interactive-hover"
              >
                <StickerIcon size={20} />
              </button>
            }
            onPick={(st) => void sendMessage(channelId, "", { stickerId: st.id })}
          />
        )}
        <EmojiPicker
          custom={customEmojis}
          trigger={
            <button title="Émoji" className="pressable text-interactive-normal hover:text-interactive-hover">
              <Smile size={20} />
            </button>
          }
          onPick={(emoji) => setText((t) => t + (t.endsWith(" ") || t === "" ? "" : " ") + emoji + " ")}
        />
        <textarea
          ref={taRef}
          rows={1}
          value={text}
          onChange={(e) => onChange(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              void submit();
            }
          }}
          placeholder={
            uploading && sendRequested
              ? "Envoi dès la fin du téléversement…"
              : `Envoyer un message dans ${title}`
          }
          className="max-h-[200px] flex-1 resize-none self-center bg-transparent py-3 text-normal outline-none scroll-thin placeholder:text-muted"
        />
      </div>

      {slowmode > 0 && !slowmodeExempt && (
        <div className="mt-1 flex items-center gap-1.5 px-1 text-xs text-muted">
          <Timer size={13} />
          {cooldownLeft > 0 ? (
            <span>
              Mode lent — attends <span className="font-medium text-normal">{cooldownLeft}s</span>{" "}
              avant ton prochain message.
            </span>
          ) : (
            <span>Mode lent activé : {formatSlowmode(slowmode)} entre deux messages.</span>
          )}
        </div>
      )}

      {pollOpen && <CreatePollModal channelId={channelId} onClose={() => setPollOpen(false)} />}
    </div>
  );
}

// Contrôles d'un fil dans l'en-tête (archiver / verrouiller) — réservés à MANAGE_CHANNELS.
function ThreadControls({ channel, guildId }: { channel: Channel; guildId: string }) {
  const canManage = useStore((s) => canIn(s, guildId, PERM.MANAGE_CHANNELS));
  if (!canManage) {
    // Indicateurs en lecture seule pour les autres.
    return channel.archived || channel.locked ? (
      <span className="text-xs text-muted">{channel.locked ? "Verrouillé" : "Archivé"}</span>
    ) : null;
  }
  async function patch(body: { archived?: boolean; locked?: boolean }) {
    try {
      const updated = await api.updateChannel(channel.id, body);
      // Met à jour le cache des fils (threadsByChannel) en direct.
      useStore.setState((s) => {
        const next: Record<string, Channel[]> = {};
        for (const [pid, list] of Object.entries(s.threadsByChannel)) {
          next[pid] = list.map((t) => (t.id === channel.id ? updated : t));
        }
        return { threadsByChannel: next };
      });
    } catch {
      /* ignore */
    }
  }
  return (
    <div className="flex items-center gap-2">
      <button
        onClick={() => void patch({ locked: !channel.locked })}
        title={channel.locked ? "Déverrouiller le fil" : "Verrouiller le fil"}
        className={`outline-none transition-colors ${
          channel.locked ? "text-dnd" : "text-interactive-normal hover:text-interactive-hover"
        }`}
      >
        <Lock size={18} />
      </button>
      <button
        onClick={() => void patch({ archived: !channel.archived })}
        title={channel.archived ? "Désarchiver le fil" : "Archiver le fil"}
        className={`outline-none transition-colors ${
          channel.archived ? "text-accent" : "text-interactive-normal hover:text-interactive-hover"
        }`}
      >
        <Archive size={18} />
      </button>
    </div>
  );
}

// Durée slowmode lisible (1s..6h).
function formatSlowmode(s: number): string {
  if (s < 60) return `${s}s`;
  if (s < 3600) return `${Math.round(s / 60)} min`;
  return `${Math.round(s / 3600)} h`;
}
